//! Sensor polling. CPU/RAM/net via sysinfo, GPU via NVML, battery via Win32,
//! CPU temp + package power via the cpu-temp crate (PawnIO driver — needs admin).
use chrono::Local;
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::Nvml;
use sysinfo::{Disks, Networks, System};

// Intel RAPL MSRs for package power.
const MSR_RAPL_POWER_UNIT: u32 = 0x606;
const MSR_PKG_ENERGY_STATUS: u32 = 0x611;
const HIST_LEN: usize = 80; // ~80s window at 1s cadence

/// One frame of sensor values shown in the TUI.
pub struct SensorSnapshot {
    pub time: String,
    pub date: String,
    pub cpu_usage: f32, // %
    pub cpu_temp: Option<f32>, // °C
    pub cpu_freq: u64, // MHz
    pub cpu_power: Option<f32>, // W (package)
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub ram_pct: f64,
    pub gpu_temp: Option<u32>, // °C
    pub gpu_usage: Option<u32>, // %
    pub gpu_vram_pct: Option<f64>, // %
    pub gpu_clock: Option<u32>, // MHz
    pub battery: Option<u8>, // %
    pub net_down: u64, // bytes/s
    pub net_up: u64, // bytes/s
    // rolling history (0..100) for the sparkline charts
    pub cpu_hist: Vec<u64>,
    pub gpu_hist: Vec<u64>,
    pub gpu_vram_hist: Vec<u64>,
    pub ram_hist: Vec<u64>,
    pub net_down_hist: Vec<u64>, // bytes/s
    pub net_up_hist: Vec<u64>, // bytes/s
    pub disks: Vec<(String, f32)>, // (label "C:", used %) — up to 3
}

/// Owns the temperature monitor + MSR accessor. `new()` loads the ring-0 driver;
/// returns None on failure (no admin / PawnIO missing / HVCI blocking).
struct CpuTempReader {
    monitor: cpu_temp::cpu::intel::IntelCpuTemperature,
    msr: cpu_temp::msr::intel_msr::IntelMsr,
    energy_unit: f64, // joules per RAPL energy LSB
    last_energy: Option<(u64, std::time::Instant)>,
}

impl CpuTempReader {
    fn new() -> Option<Self> {
        let monitor = cpu_temp::cpu::intel::IntelCpuTemperature::new().ok()?;
        let msr = cpu_temp::msr::intel_msr::IntelMsr::new().ok()?;
        let energy_unit = msr
            .read_msr(MSR_RAPL_POWER_UNIT)
            .ok()
            .map(|v| {
                let eu = ((v >> 8) & 0x1f) as u32;
                1.0 / (1u64 << eu) as f64
            })
            .unwrap_or(1.0 / 16384.0); // typical 2^-14 fallback
        Some(Self { monitor, msr, energy_unit, last_energy: None })
    }

    fn read_temp(&mut self) -> Option<f32> {
        let data = self.monitor.get_temperatures(&mut self.msr).ok()?;
        if let Ok(p) = &data.package_temp {
            return Some(p.temperature);
        }
        let mut max: Option<f32> = None;
        for c in &data.core_temps {
            max = Some(max.map_or(c.core_temp.temperature, |m| m.max(c.core_temp.temperature)));
        }
        max
    }

    /// Package power in watts (delta-energy / delta-time from the last read).
    fn read_power(&mut self) -> Option<f32> {
        let e = self.msr.read_msr(MSR_PKG_ENERGY_STATUS).ok()? & 0xFFFFFFFF;
        let now = std::time::Instant::now();
        let watts = if let Some((le, lt)) = self.last_energy {
            let dt = now.duration_since(lt).as_secs_f64();
            if dt > 0.0 {
                let mut de = e as i64 - le as i64;
                if de < 0 {
                    de += 1i64 << 32; // counter wrapped
                }
                Some(de as f64 * self.energy_unit / dt)
            } else {
                None
            }
        } else {
            None
        };
        self.last_energy = Some((e, now));
        watts.map(|w| w as f32)
    }
}

/// Owns all long-lived handles; call `poll()` each tick.
pub struct Sensors {
    sys: System,
    nets: Networks,
    nvml: Option<Nvml>,
    cpu: Option<CpuTempReader>,
    cpu_hist: Vec<u64>,
    gpu_hist: Vec<u64>,
    gpu_vram_hist: Vec<u64>,
    ram_hist: Vec<u64>,
    net_down_hist: Vec<u64>,
    net_up_hist: Vec<u64>,
    disks: Disks,
}

impl Sensors {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_specifics(sysinfo::CpuRefreshKind::everything());
        Sensors {
            sys,
            nets: Networks::new_with_refreshed_list(),
            nvml: Nvml::init().ok(),
            cpu: CpuTempReader::new(),
            cpu_hist: Vec::new(),
            gpu_hist: Vec::new(),
            gpu_vram_hist: Vec::new(),
            ram_hist: Vec::new(),
            net_down_hist: Vec::new(),
            net_up_hist: Vec::new(),
            disks: Disks::new_with_refreshed_list(),
        }
    }

    pub fn has_cpu(&self) -> bool {
        self.cpu.is_some()
    }
    pub fn has_nvml(&self) -> bool {
        self.nvml.is_some()
    }

    pub fn poll(&mut self) -> SensorSnapshot {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.nets.refresh(true);

        let now = Local::now();
        let cpu_usage = self.sys.global_cpu_usage();
        let cpu_freq = self.sys.cpus().first().map(|c| c.frequency()).unwrap_or(0);

        let total = self.sys.total_memory() as f64;
        let used = self.sys.used_memory() as f64;
        let ram_total_gb = total / 1_073_741_824.0;
        let ram_used_gb = used / 1_073_741_824.0;
        let ram_pct = if total > 0.0 { used / total * 100.0 } else { 0.0 };

        let (cpu_temp, cpu_power) = match self.cpu.as_mut() {
            Some(r) => (r.read_temp(), r.read_power()),
            None => (None, None),
        };

        let (gpu_temp, gpu_usage, gpu_vram_pct, gpu_clock) = match &self.nvml {
            Some(nvml) => match nvml.device_by_index(0) {
                Ok(dev) => {
                    let t = dev.temperature(TemperatureSensor::Gpu).ok();
                    let u = dev.utilization_rates().ok().map(|u| u.gpu);
                    let v = dev.memory_info().ok().and_then(|m| {
                        if m.total > 0 {
                            Some(m.used as f64 / m.total as f64 * 100.0)
                        } else {
                            None
                        }
                    });
                    let c = dev.clock_info(Clock::Graphics).ok();
                    (t, u, v, c)
                }
                Err(_) => (None, None, None, None),
            },
            None => (None, None, None, None),
        };

        let battery = read_battery();

        let (mut down, mut up) = (0u64, 0u64);
        for (_, data) in &self.nets {
            down += data.received();
            up += data.transmitted();
        }

        // Top-3 disks by total size → (label "C:", used %).
        self.disks.refresh(true);
        let mut disk_list: Vec<(String, f32, u64)> = self
            .disks
            .list()
            .iter()
            .filter_map(|d| {
                let total = d.total_space();
                if total < 1_000_000_000 {
                    return None; // skip tiny volumes (< 1 GB)
                }
                let used = total.saturating_sub(d.available_space());
                let pct = (used as f64 / total as f64 * 100.0) as f32;
                let label: String = d.mount_point().to_string_lossy().chars().take(2).collect();
                Some((label, pct, total))
            })
            .collect();
        disk_list.sort_by(|a, b| a.0.cmp(&b.0)); // C, D, E … drive-letter order
        let disks: Vec<(String, f32)> =
            disk_list.into_iter().take(3).map(|(l, p, _)| (l, p)).collect();

        push_hist(&mut self.cpu_hist, cpu_usage.round().clamp(0.0, 100.0) as u64);
        push_hist(&mut self.gpu_hist, gpu_usage.unwrap_or(0) as u64);
        push_hist(&mut self.gpu_vram_hist, gpu_vram_pct.unwrap_or(0.0).round().clamp(0.0, 100.0) as u64);
        push_hist(&mut self.ram_hist, ram_pct.round().clamp(0.0, 100.0) as u64);
        push_hist_raw(&mut self.net_down_hist, down);
        push_hist_raw(&mut self.net_up_hist, up);

        SensorSnapshot {
            time: now.format("%H:%M").to_string(),
            date: now.format("%m月%d日").to_string(),
            cpu_usage,
            cpu_temp,
            cpu_freq,
            cpu_power,
            ram_used_gb,
            ram_total_gb,
            ram_pct,
            gpu_temp,
            gpu_usage,
            gpu_vram_pct,
            gpu_clock,
            battery,
            net_down: down,
            net_up: up,
            cpu_hist: self.cpu_hist.clone(),
            gpu_hist: self.gpu_hist.clone(),
            gpu_vram_hist: self.gpu_vram_hist.clone(),
            ram_hist: self.ram_hist.clone(),
            net_down_hist: self.net_down_hist.clone(),
            net_up_hist: self.net_up_hist.clone(),
            disks,
        }
    }
}

fn push_hist(h: &mut Vec<u64>, v: u64) {
    h.push(v.min(100));
    if h.len() > HIST_LEN {
        h.remove(0);
    }
}

/// Rolling history without clamping (for net speeds in bytes/s).
fn push_hist_raw(h: &mut Vec<u64>, v: u64) {
    h.push(v);
    if h.len() > HIST_LEN {
        h.remove(0);
    }
}

fn read_battery() -> Option<u8> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut sps = SYSTEM_POWER_STATUS::default();
    unsafe {
        let _ = GetSystemPowerStatus(&mut sps);
    }
    if sps.BatteryLifePercent == 255 {
        None
    } else {
        Some(sps.BatteryLifePercent)
    }
}

pub fn fmt_speed(bps: u64) -> String {
    let mb = bps as f64 / 1_048_576.0;
    if mb >= 1.0 {
        format!("{:.1} MB/s", mb)
    } else {
        format!("{:.0} KB/s", bps as f64 / 1024.0)
    }
}

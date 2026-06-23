//! hud-cli — native Windows system dashboard (egui), no Rainmeter/HWiNFO.
//! Opens a borderless window sized to the 1424x280 HUD monitor. CPU temp/power via
//! the cpu-temp PawnIO driver (needs admin). Press the window's close or Esc to quit.
//!
//! Usage:
//!   hud-cli            native GUI dashboard on the HUD screen
//!   hud-cli --once     poll once and print values to the console (no GUI)

mod app;
mod sensors;
mod theme;
mod window;

use std::sync::Arc;
use std::time::Duration;

fn main() -> eframe::Result {
    let once = std::env::args().any(|a| a == "--once");
    if once {
        return run_once();
    }

    let Some((x, y, w, h)) = window::hud_physical() else {
        eprintln!("未检测到分辨率 1424×280 的 HUD 屏幕，退出。");
        return Ok(());
    };

    window::hide_console(); // GUI mode: the egui window is the only thing visible.

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_position(egui::pos2(x as f32, y as f32))
            .with_inner_size(egui::vec2(w as f32, h as f32))
            .with_resizable(false)
            .with_active(true),
        ..Default::default()
    };

    eframe::run_native(
        "MagicBay HUD",
        opts,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc, x, y, w, h)))
        }),
    )
}

/// Load Microsoft YaHei so Chinese labels render (egui's default font has no CJK).
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\msyh.ttc") {
        fonts
            .font_data
            .insert("yahei".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            fam.insert(0, "yahei".to_owned());
        }
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            fam.push("yahei".to_owned());
        }
    }
    // emoji fallback (Segoe UI Emoji) so symbols like 🔋 render
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\seguiemj.ttf") {
        fonts
            .font_data
            .insert("emoji".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            fam.push("emoji".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// Bump the default text sizes for HUD legibility, and force dark visuals so
/// plot backgrounds / progress-bar troughs / axis labels match the theme.
fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let f = |size: f32| egui::FontId::new(size, egui::FontFamily::Proportional);
    style.text_styles.insert(egui::TextStyle::Body, f(30.0));
    style.text_styles.insert(egui::TextStyle::Button, f(22.0));
    style.text_styles.insert(egui::TextStyle::Small, f(18.0));
    style.text_styles.insert(egui::TextStyle::Heading, f(32.0));
    style.text_styles.insert(egui::TextStyle::Monospace, f(19.0));
    ctx.set_style(style);
    let mut visuals = egui::Visuals::dark();
    visuals.extreme_bg_color = egui::Color32::from_rgb(14, 16, 24); // darker than panel — no bright trough
    ctx.set_visuals(visuals);
}

/// Diagnostic: poll twice (so CPU-usage / power deltas are meaningful) and print.
fn run_once() -> eframe::Result {
    let mut sensors = sensors::Sensors::new();
    if !sensors.has_cpu() {
        eprintln!("⚠ CPU 温度/功率未读到：需 PawnIO 驱动 + 管理员。其余指标正常。");
    }
    if !sensors.has_nvml() {
        eprintln!("⚠ NVIDIA NVML 初始化失败。GPU 指标不可用。");
    }
    let _ = sensors.poll();
    std::thread::sleep(Duration::from_millis(1100));
    let s = sensors.poll();
    println!("================  hud-cli  ================");
    println!("时间   {}  {}", s.time, s.date);
    println!("CPU    占用 {:.0}%  频率 {}  温度 {}  功率 {}",
        s.cpu_usage,
        if s.cpu_freq > 0 { format!("{}MHz", s.cpu_freq) } else { "N/A".into() },
        s.cpu_temp.map(|t| format!("{:.0}°C", t)).unwrap_or_else(|| "N/A".into()),
        s.cpu_power.map(|p| format!("{:.1}W", p)).unwrap_or_else(|| "N/A".into()));
    println!("内存   {:.1}/{:.1}GB ({:.0}%)", s.ram_used_gb, s.ram_total_gb, s.ram_pct);
    println!("GPU    温度 {}  占用 {}  显存 {}  频率 {}",
        s.gpu_temp.map(|t| format!("{}°C", t)).unwrap_or_else(|| "N/A".into()),
        s.gpu_usage.map(|u| format!("{}%", u)).unwrap_or_else(|| "N/A".into()),
        s.gpu_vram_pct.map(|v| format!("{:.0}%", v)).unwrap_or_else(|| "N/A".into()),
        s.gpu_clock.map(|c| format!("{}MHz", c)).unwrap_or_else(|| "N/A".into()));
    println!("电池   {}", s.battery.map(|b| format!("{}%", b)).unwrap_or_else(|| "N/A".into()));
    println!("网速   ↓ {}  ↑ {}", sensors::fmt_speed(s.net_down), sensors::fmt_speed(s.net_up));
    println!("==========================================");
    Ok(())
}

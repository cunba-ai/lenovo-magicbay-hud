//! egui dashboard: 5 cards (时钟/电池 · CPU · 内存 · GPU · 网速) that fill the HUD.
//! Each usage metric has a progress bar and a time-series sparkline; net shows
//! separate download/upload sparklines.
use std::time::{Duration, Instant};

use eframe::egui;
use egui::Color32;

use crate::sensors::{fmt_speed, SensorSnapshot, Sensors};
use crate::theme;

pub struct App {
    sensors: Sensors,
    snap: SensorSnapshot,
    last_poll: Instant,
    hud: (i32, i32, i32, i32),
    place_attempts: u32,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>, x: i32, y: i32, w: i32, h: i32) -> Self {
        let mut sensors = Sensors::new();
        let snap = sensors.poll();
        Self {
            sensors,
            snap,
            last_poll: Instant::now(),
            hud: (x, y, w, h),
            place_attempts: 0,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Re-apply every frame: eframe resets visuals/style from its theme,
        // which previously overrode our font sizes (and lit up bar troughs).
        apply_style(ui.ctx());
        // Full-window opaque black so the inset margins (around the cards) don't
        // show the desktop through the borderless window's transparency.
        ui.painter()
            .rect_filled(ui.ctx().screen_rect(), 0.0, egui::Color32::BLACK);
        // Force the borderless window onto the HUD monitor (raw SetWindowPos —
        // eframe's with_position/with_inner_size are unreliable on multi-monitor).
        if self.place_attempts < 5 {
            let (x, y, w, h) = self.hud;
            crate::window::place_window(x, y, w, h);
            self.place_attempts += 1;
        }
        // Esc closes the (borderless) window.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.last_poll.elapsed() >= Duration::from_millis(1000) {
            self.snap = self.sensors.poll();
            self.last_poll = Instant::now();
        }
        ui.ctx().request_repaint_after(Duration::from_millis(500));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::bg()).inner_margin(egui::Margin::same(4)))
            .show_inside(ui, |ui| {
                // Inset the card grid so the card borders clear the screen's
                // bottom/right non-visible edge (bezel) as well as the panel clip.
                // More on bottom/right (the HUD panel clips those sides).
                let top = 2.0;
                let left = 2.0;
                let bottom = 2.0;
                let right = 10.0;
                let panel_w = ui.available_width();
                let panel_h = ui.available_height();
                let card_h = (panel_h - top - bottom).max(0.0);
                ui.add_space(top);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
                    ui.add_space(left);
                    let gap = 12.0_f32;
                    let card_w = ((panel_w - left - right) - gap * 4.0) / 5.0;
                    let size = egui::vec2(card_w, card_h);
                    let layout = egui::Layout::top_down(egui::Align::LEFT);
                    ui.allocate_ui_with_layout(size, layout, |ui| self.time_card(ui));
                    ui.allocate_ui_with_layout(size, layout, |ui| self.cpu_card(ui));
                    ui.allocate_ui_with_layout(size, layout, |ui| self.mem_card(ui));
                    ui.allocate_ui_with_layout(size, layout, |ui| self.gpu_card(ui));
                    ui.allocate_ui_with_layout(size, layout, |ui| self.net_card(ui));
                });
            });
    }
}

impl App {
    fn time_card(&self, ui: &mut egui::Ui) {
        let snap = &self.snap;
        card(ui, "时钟 / 磁盘 / 电池", theme::time(), |ui| {
            ui.label(
                egui::RichText::new(&snap.time)
                    .color(theme::time())
                    .size(44.0)
                    .strong(),
            );
            ui.label(egui::RichText::new(&snap.date).color(theme::dim()).size(16.0));
            // disks + battery grouped at the bottom; overestimate group_h so they never overflow
            let group_h = 110.0;
            let rem = (ui.available_height() - group_h).max(0.0);
            ui.add_space(rem);
            // Fixed-width label column so all four bars start at the same x and share width.
            let label_w = 64.0;
            let label_row = |ui: &mut egui::Ui, text: String| {
                ui.add_sized(
                    egui::vec2(label_w, 20.0),
                    egui::Label::new(egui::RichText::new(text).size(13.0).color(theme::val())),
                );
            };
            for (i, (label, pct)) in snap.disks.iter().take(3).enumerate() {
                if i > 0 {
                    ui.add_space(5.0);
                }
                ui.horizontal(|ui| {
                    label_row(ui, format!("{} {:.0}%", label, pct));
                    bar(ui, *pct / 100.0, 20.0);
                });
            }
            ui.add_space(5.0); // gap before battery, same as between disks
            ui.horizontal(|ui| {
                label_row(ui, format!("🔋 {}%", snap.battery.map(|b| b.to_string()).unwrap_or_else(|| "N/A".into())));
                bar(ui, snap.battery.unwrap_or(0) as f32 / 100.0, 20.0);
            });
        });
    }

    fn cpu_card(&self, ui: &mut egui::Ui) {
        let snap = &self.snap;
        card(ui, "CPU", theme::cpu(), |ui| {
            row(ui, "频率", if snap.cpu_freq > 0 { format!("{} MHz", snap.cpu_freq) } else { "N/A".into() });
            row(ui, "核心温度", snap.cpu_temp.map(|t| format!("{:.0} °C", t)).unwrap_or_else(|| "N/A".into()));
            row(ui, "功率", snap.cpu_power.map(|p| format!("{:.1} W", p)).unwrap_or_else(|| "N/A".into()));
            ui.add_space(2.0);
            bar(ui, snap.cpu_usage / 100.0, 24.0);
            ui.add_space((ui.available_height() - 76.0).max(0.0));
            sparkline(ui, &snap.cpu_hist, theme::cpu(), "cpu_spark", 64.0, 1.0, true, None);
        });
    }

    fn mem_card(&self, ui: &mut egui::Ui) {
        let snap = &self.snap;
        card(ui, "内存", theme::ram(), |ui| {
            row(ui, "已用", format!("{:.1} GB", snap.ram_used_gb));
            row(ui, "空闲", format!("{:.1} GB", snap.ram_total_gb - snap.ram_used_gb));
            row(ui, "总计", format!("{:.1} GB", snap.ram_total_gb));
            ui.add_space(2.0);
            bar(ui, snap.ram_pct as f32 / 100.0, 24.0);
            ui.add_space((ui.available_height() - 76.0).max(0.0));
            sparkline(ui, &snap.ram_hist, theme::ram(), "ram_spark", 64.0, 1.0, true, None);
        });
    }

    fn gpu_card(&self, ui: &mut egui::Ui) {
        let snap = &self.snap;
        card(ui, "GPU", theme::gpu(), |ui| {
            row(ui, "温度", snap.gpu_temp.map(|t| format!("{} °C", t)).unwrap_or_else(|| "N/A".into()));
            row(ui, "显存", snap.gpu_vram_pct.map(|v| format!("{:.0}%", v)).unwrap_or_else(|| "N/A".into()));
            row(ui, "频率", snap.gpu_clock.map(|c| format!("{} MHz", c)).unwrap_or_else(|| "N/A".into()));
            ui.add_space(2.0);
            bar(ui, snap.gpu_usage.unwrap_or(0) as f32 / 100.0, 24.0);
            ui.add_space((ui.available_height() - 76.0).max(0.0));
            // usage (orange) + VRAM (purple) on one chart, both 0–100
            sparkline(
                ui,
                &snap.gpu_hist,
                theme::gpu(),
                "gpu_spark",
                64.0,
                1.0,
                true,
                Some((&snap.gpu_vram_hist, theme::ram())),
            );
        });
    }

    fn net_card(&self, ui: &mut egui::Ui) {
        let snap = &self.snap;
        card(ui, "网速", theme::net(), |ui| {
            row(ui, "↓ 下载", fmt_speed(snap.net_down));
            row(ui, "↑ 上传", fmt_speed(snap.net_up));
            // two charts split the remaining height; -12 absorbs each plot's render overhead
            let h = ((ui.available_height() - 12.0) / 2.0).max(20.0);
            sparkline(ui, &snap.net_down_hist, theme::net(), "net_down_spark", h, 1.0 / 1_048_576.0, false, None);
            sparkline(ui, &snap.net_up_hist, theme::val(), "net_up_spark", h, 1.0 / 1024.0, false, None);
        });
    }
}

/// Re-apply text sizes + dark visuals. Called every frame because eframe resets
/// style from its theme, which previously overrode our font sizes.
fn apply_style(ctx: &egui::Context) {
    // Skip the (repaint-triggering) set when the style is already ours — calling
    // set_style/set_visuals every frame caused continuous repaints and memory growth.
    let cur = ctx.style();
    let body_ok = cur
        .text_styles
        .get(&egui::TextStyle::Body)
        .map_or(false, |f| (f.size - 25.0).abs() < 0.1);
    if body_ok && cur.visuals.dark_mode {
        return;
    }
    let mut style = (*cur).clone();
    let f = |s: f32| egui::FontId::new(s, egui::FontFamily::Proportional);
    style.text_styles.insert(egui::TextStyle::Body, f(25.0));
    style.text_styles.insert(egui::TextStyle::Button, f(25.0));
    style.text_styles.insert(egui::TextStyle::Small, f(21.0));
    style.text_styles.insert(egui::TextStyle::Heading, f(33.0));
    style.text_styles.insert(egui::TextStyle::Monospace, f(23.0));
    ctx.set_style(style);
    let mut visuals = egui::Visuals::dark();
    visuals.extreme_bg_color = egui::Color32::from_rgb(14, 16, 24);
    ctx.set_visuals(visuals);
}

/// A bordered panel painted at the card's allocated rect (fixed size, so all cards
/// are identical and align), with content clipped to the inner area.
fn card(ui: &mut egui::Ui, title: &str, color: Color32, add: impl FnOnce(&mut egui::Ui)) {
    let size = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let frame = egui::Frame::group(ui.style())
        .fill(theme::panel())
        .stroke(egui::Stroke::new(1.5, color))
        .corner_radius(10.0)
        .inner_margin(8.0);
    let inner = rect.shrink(8.0);
    ui.painter().add(frame.paint(inner));
    let mut content = ui.child_ui_with_id_source(
        inner,
        egui::Layout::top_down(egui::Align::LEFT),
        title,
        None,
    );
    content.set_clip_rect(inner);
    content.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
    content.label(egui::RichText::new(title).color(color).size(16.0).strong());
    content.separator();
    add(&mut content);
}

fn row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.colored_label(theme::dim(), label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.colored_label(theme::val(), value);
        });
    });
}

/// Custom progress bar: rectangular, dark trough, a green→yellow→red gradient
/// fill (discrete bands with small gaps) clipped to the fraction. No text.
fn bar(ui: &mut egui::Ui, fraction: f32, height: f32) {
    let frac = fraction.clamp(0.0, 1.0);
    let width = ui.available_width().max(0.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 24, 34)); // dark trough
    if frac > 0.0 && rect.width() > 0.0 {
        let full_w = rect.width();
        let band_w = 10.0;
        let gap = 2.0;
        let step = band_w + gap;
        let n = ((full_w / step).ceil() as i32).max(1);
        let fill_x = rect.left() + full_w * frac;
        for i in 0..n {
            let left = rect.left() + i as f32 * step;
            if left >= fill_x {
                break;
            }
            let right = (left + band_w).min(fill_x).min(rect.right());
            if right <= left {
                break;
            }
            let t = (left - rect.left()) / full_w;
            let seg = egui::Rect::from_min_max(
                egui::pos2(left, rect.top()),
                egui::pos2(right, rect.bottom()),
            );
            painter.rect_filled(seg, 0.0, load_grad(t));
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Load colour at position t in [0,1]: green → (brief) yellow → red. The
/// green→yellow transition is short (0..0.3) so red dominates the high end.
fn load_grad(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.3 {
        let k = t / 0.3;
        (lerp(70.0, 235.0, k), lerp(210.0, 200.0, k), lerp(110.0, 60.0, k))
    } else {
        let k = (t - 0.3) / 0.7;
        (lerp(235.0, 225.0, k), lerp(200.0, 75.0, k), lerp(60.0, 70.0, k))
    };
    Color32::from_rgb(r as u8, g as u8, b as u8)
}

/// Sparkline with a Y axis (ticks + labels). `scale` converts raw values (e.g.
/// bytes/s → MB/s); `pct` fixes the 0–100 range for CPU/GPU/RAM usage charts.
/// `second` draws a second series on the same plot (e.g. GPU VRAM alongside usage).
fn sparkline(
    ui: &mut egui::Ui,
    hist: &[u64],
    color: Color32,
    id: &str,
    height: f32,
    scale: f64,
    pct: bool,
    second: Option<(&[u64], Color32)>,
) {
    let mk = |h: &[u64]| {
        egui_plot::PlotPoints::from_iter(h.iter().enumerate().map(|(i, v)| [i as f64, *v as f64 * scale]))
    };
    let mut plot = egui_plot::Plot::new(id)
        .show_background(false) // transparent → card panel shows through
        .show_axes([false, true]) // show Y axis, hide X
        .show_grid([false, false])
        .height(height)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_drag(false)
        .allow_boxed_zoom(false)
        .allow_axis_zoom_drag(false)
        .allow_double_click_reset(false);
    if pct {
        plot = plot.include_y(0.0).include_y(100.0);
    }
    plot.show(ui, |pui| {
        pui.line(egui_plot::Line::new(id, mk(hist)).color(color).width(1.5));
        if let Some((h2, c2)) = second {
            pui.line(egui_plot::Line::new(format!("{}2", id), mk(h2)).color(c2).width(1.5));
        }
    });
}

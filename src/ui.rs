//! ratatui dashboard: 4 cards (时钟/电池 · CPU · 内存 · GPU) with time-series
//! sparklines for the usage metrics, plus a network bar.
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Sparkline};
use ratatui::Frame;

use crate::sensors::{fmt_speed, SensorSnapshot};
use crate::theme;

pub fn draw(f: &mut Frame, s: &SensorSnapshot) {
    let total = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(total);
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(chunks[0]);

    draw_time_card(f, cards[0], s);
    metric_card(
        f,
        cards[1],
        "CPU",
        theme::cpu(),
        &[
            ("频率", freq_str(s.cpu_freq)),
            ("核心温度", s.cpu_temp.map(|t| format!("{:.0} °C", t)).unwrap_or_else(|| "N/A".into())),
            ("功率", s.cpu_power.map(|p| format!("{:.1} W", p)).unwrap_or_else(|| "N/A".into())),
        ],
        Some((&s.cpu_hist, format!("使用率 {:.0}%", s.cpu_usage))),
    );
    metric_card(
        f,
        cards[2],
        "内存",
        theme::ram(),
        &[
            ("已用", format!("{:.1} GB", s.ram_used_gb)),
            ("总计", format!("{:.1} GB", s.ram_total_gb)),
        ],
        Some((&s.ram_hist, format!("使用率 {:.0}%", s.ram_pct))),
    );
    metric_card(
        f,
        cards[3],
        "GPU",
        theme::gpu(),
        &[
            ("温度", s.gpu_temp.map(|t| format!("{} °C", t)).unwrap_or_else(|| "N/A".into())),
            ("显存", s.gpu_vram_pct.map(|v| format!("{:.0}%", v)).unwrap_or_else(|| "N/A".into())),
            ("频率", s.gpu_clock.map(|c| format!("{} MHz", c)).unwrap_or_else(|| "N/A".into())),
        ],
        Some((&s.gpu_hist, format!("占用 {}", s.gpu_usage.map(|u| format!("{}%", u)).unwrap_or_else(|| "N/A".into())))),
    );
    draw_net(f, chunks[1], s);
}

fn freq_str(f: u64) -> String {
    if f > 0 {
        format!("{} MHz", f)
    } else {
        "N/A".into()
    }
}

fn block(title: &str, color: Color) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(Line::from(Span::styled(
            format!(" {} ", title),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )))
}

fn row<'a>(label: &'a str, value: &'a str, color: Color) -> Line<'a> {
    Line::from(vec![
        Span::raw(format!("{} ", label)),
        Span::styled(value.to_string(), Style::default().fg(color)),
    ])
}

/// A bordered card: metric rows on top, a time-series sparkline below (when given).
fn metric_card(
    f: &mut Frame,
    area: Rect,
    title: &str,
    color: Color,
    rows: &[(&str, String)],
    spark: Option<(&[u64], String)>,
) {
    let b = block(title, color);
    let inner = b.inner(area);
    f.render_widget(b, area);
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(8)])
        .split(inner);
    let lines: Vec<Line> = rows.iter().map(|(l, v)| row(l, v, theme::val())).collect();
    f.render_widget(Paragraph::new(lines), split[0]);
    if let Some((hist, label)) = spark {
        let chart_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme::dim()))
            .title(Line::from(Span::styled(format!(" {} ", label), Style::default().fg(color))));
        f.render_widget(
            Sparkline::default().block(chart_block).data(hist).style(Style::default().fg(color)),
            split[1],
        );
    }
}

fn draw_time_card(f: &mut Frame, area: Rect, s: &SensorSnapshot) {
    let b = block("时钟 / 电池", theme::time());
    let inner = b.inner(area);
    f.render_widget(b, area);
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);
    let bat_str = s.battery.map(|p| format!("{}%", p)).unwrap_or_else(|| "N/A".into());
    let lines = vec![
        Line::from(Span::styled(
            s.time.clone(),
            Style::default().fg(theme::time()).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(s.date.clone(), Style::default().fg(theme::dim()))),
        Line::from(""),
        row("电池", &bat_str, theme::bat()),
    ];
    f.render_widget(Paragraph::new(lines), split[0]);
    f.render_widget(
        ratatui::widgets::Gauge::default()
            .ratio(s.battery.map(|p| p as f64 / 100.0).unwrap_or(0.0))
            .gauge_style(Style::default().fg(theme::bat())),
        split[1],
    );
}

fn draw_net(f: &mut Frame, area: Rect, s: &SensorSnapshot) {
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::net()))
        .title(Line::from(Span::styled(
            " 网络 ",
            Style::default().fg(theme::net()).add_modifier(Modifier::BOLD),
        )));
    let inner = b.inner(area);
    f.render_widget(b, area);
    let line = Line::from(vec![
        Span::styled(format!(" ↓ {}  ", fmt_speed(s.net_down)), Style::default().fg(theme::net())),
        Span::styled(format!("↑ {}", fmt_speed(s.net_up)), Style::default().fg(theme::val())),
    ]);
    f.render_widget(Paragraph::new(line), inner);
}

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::app::App;

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1}GB/s", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1}MB/s", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1}KB/s", bytes as f64 / 1_000.0)
    } else {
        format!("{}B/s", bytes)
    }
}

fn metric_color(percent: f64) -> Color {
    if percent >= 90.0 {
        Color::Red
    } else if percent >= 70.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

pub struct MetricsStrip<'a> {
    pub app: &'a App,
}

impl<'a> Widget for MetricsStrip<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let m = &self.app.system_metrics;
        let s = &self.app.session_counts;

        let bold = Style::default().add_modifier(Modifier::BOLD);
        let dim = Style::default().fg(Color::DarkGray);

        let sep = Span::styled(" │ ", Style::default().fg(Color::Rgb(50, 50, 60)));

        let mut spans = vec![
            Span::styled(" CPU:", dim),
            Span::styled(
                format!("{:.0}%", m.cpu_percent),
                bold.fg(metric_color(m.cpu_percent)),
            ),
            sep.clone(),
            Span::styled("MEM:", dim),
            Span::styled(
                format!("{:.0}%", m.mem_percent),
                bold.fg(metric_color(m.mem_percent)),
            ),
            sep.clone(),
            Span::styled("DISK:", dim),
            Span::styled(
                format!("{:.0}%", m.disk_percent),
                bold.fg(metric_color(m.disk_percent)),
            ),
            sep.clone(),
            Span::styled("NET:", dim),
            Span::styled(format_bytes(m.net_bytes_sec), bold.fg(Color::Cyan)),
        ];

        if area.width > 70 {
            spans.push(sep.clone());
            spans.push(Span::styled(
                format!(
                    "ssh:{} mosh:{} tmux:{} claude:{}",
                    s.ssh, s.mosh, s.tmux, s.claude
                ),
                Style::default().fg(Color::Rgb(120, 120, 140)),
            ));
        }

        let line = Line::from(spans);
        let style = Style::default().bg(Color::Rgb(20, 20, 30));
        buf.set_style(area, style);
        line.render(area, buf);
    }
}

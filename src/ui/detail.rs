use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

use crate::app::{ActivePane, App, DetailTab};
use crate::providers::{LogLevel, Service, ServiceStatus};

fn format_duration(secs: u64) -> String {
    if secs >= 86400 {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    } else if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

fn format_mem(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1}GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.1}KB", bytes as f64 / 1_024.0)
    } else {
        format!("{}B", bytes)
    }
}

fn status_label(status: ServiceStatus) -> (&'static str, Color) {
    match status {
        ServiceStatus::Running => ("Running", Color::Green),
        ServiceStatus::Stopped => ("Stopped", Color::Red),
        ServiceStatus::Degraded => ("Degraded", Color::Yellow),
        ServiceStatus::Unknown => ("Unknown", Color::DarkGray),
    }
}

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let is_active = app.active_pane == ActivePane::Detail;
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            " Detail ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    // Tab bar at top
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    render_tab_bar(app, chunks[0], buf);

    let services = app.filtered_services();
    let selected = if services.is_empty() {
        None
    } else {
        services
            .get(app.selected_index.min(services.len() - 1))
            .copied()
    };

    match app.detail_tab {
        DetailTab::Info => render_info(selected, chunks[1], buf),
        DetailTab::Logs => render_logs(app, chunks[1], buf),
        DetailTab::Actions => render_actions(chunks[1], buf),
    }
}

fn render_tab_bar(app: &App, area: Rect, buf: &mut Buffer) {
    let tabs = [
        (DetailTab::Info, "i:Info"),
        (DetailTab::Logs, "l:Logs"),
        (DetailTab::Actions, "?:Keys"),
    ];

    let active_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(Color::DarkGray);

    let spans: Vec<Span> = tabs
        .iter()
        .flat_map(|(tab, label)| {
            let style = if *tab == app.detail_tab {
                active_style
            } else {
                inactive_style
            };
            vec![Span::styled(format!(" {} ", label), style), Span::raw(" ")]
        })
        .collect();

    Line::from(spans).render(area, buf);
}

fn render_info(service: Option<&Service>, area: Rect, buf: &mut Buffer) {
    let lines = if let Some(svc) = service {
        let (status_label, status_color) = status_label(svc.status);
        let pid_str = svc
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "N/A".to_string());
        let uptime_str = svc
            .uptime_secs
            .map(format_duration)
            .unwrap_or_else(|| "N/A".to_string());
        let mem_str = svc
            .memory_bytes
            .map(format_mem)
            .unwrap_or_else(|| "N/A".to_string());
        let cpu_str = svc
            .cpu_percent
            .map(|c| format!("{:.1}%", c))
            .unwrap_or_else(|| "N/A".to_string());

        let label_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);
        let value_style = Style::default().fg(Color::White);

        vec![
            Line::from(vec![
                Span::styled("  Name    : ", label_style),
                Span::styled(svc.name.clone(), value_style.add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  Status  : ", label_style),
                Span::styled(status_label, value_style.fg(status_color)),
            ]),
            Line::from(vec![
                Span::styled("  PID     : ", label_style),
                Span::styled(pid_str, value_style),
            ]),
            Line::from(vec![
                Span::styled("  Uptime  : ", label_style),
                Span::styled(uptime_str, value_style),
            ]),
            Line::from(vec![
                Span::styled("  Memory  : ", label_style),
                Span::styled(mem_str, value_style),
            ]),
            Line::from(vec![
                Span::styled("  CPU     : ", label_style),
                Span::styled(cpu_str, value_style),
            ]),
            Line::from(vec![
                Span::styled("  Type    : ", label_style),
                Span::styled(svc.provider.to_string(), value_style),
            ]),
            Line::from(vec![
                Span::styled("  ID      : ", label_style),
                Span::styled(svc.id.clone(), value_style.fg(Color::DarkGray)),
            ]),
        ]
    } else {
        vec![Line::from(Span::styled(
            "  No service selected",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    Paragraph::new(lines).render(area, buf);
}

fn render_logs(app: &App, area: Rect, buf: &mut Buffer) {
    if app.logs.is_empty() {
        // Show hint about auto-scroll in the footer
        let lines = vec![
            Line::from(Span::styled(
                "  No logs available",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Press Space to toggle auto-scroll",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        Paragraph::new(lines).render(area, buf);
        return;
    }

    let visible_height = area.height as usize;

    // Determine which entries to show based on scroll position / auto-scroll
    let total = app.logs.len();
    let start = if app.log_auto_scroll {
        // Show the last N lines
        total.saturating_sub(visible_height)
    } else {
        app.log_scroll.min(total.saturating_sub(1))
    };

    let items: Vec<ListItem> = app.logs[start..]
        .iter()
        .map(|entry| {
            let (level_str, level_color) = match entry.level {
                LogLevel::Debug => ("DBG", Color::DarkGray),
                LogLevel::Info => ("INF", Color::Blue),
                LogLevel::Warn => ("WRN", Color::Yellow),
                LogLevel::Error => ("ERR", Color::Red),
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("[{}] ", level_str),
                    Style::default()
                        .fg(level_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(entry.message.clone(), Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    // Auto-scroll status indicator at bottom right
    let auto_str = if app.log_auto_scroll {
        " [AUTO] "
    } else {
        " [PAUSED] "
    };
    let scroll_hint = Line::from(Span::styled(
        format!("{}(Space to toggle)", auto_str),
        Style::default().fg(Color::DarkGray),
    ));

    // Split area: logs + 1 line status
    if area.height > 1 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        List::new(items).render(chunks[0], buf);
        Paragraph::new(scroll_hint).render(chunks[1], buf);
    } else {
        List::new(items).render(area, buf);
    }
}

fn render_actions(area: Rect, buf: &mut Buffer) {
    let label_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);

    let lines = vec![
        Line::from(vec![
            Span::styled("  S ", label_style),
            Span::styled("Start service (stopped/degraded only)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  s ", label_style),
            Span::styled("Stop service (running only, confirms)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  r ", label_style),
            Span::styled("Restart service (confirms)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  e ", label_style),
            Span::styled("Enable service (systemd only)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  d ", label_style),
            Span::styled("Disable service (systemd only, confirms)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  f ", label_style),
            Span::styled("Cycle filter (All/Systemd/Docker/PM2)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Tab ", label_style),
            Span::styled("Switch pane", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  ? ", label_style),
            Span::styled("Help overlay", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  ^P ", label_style),
            Span::styled("Command palette", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  q ", label_style),
            Span::styled("Quit", desc_style),
        ]),
    ];

    Paragraph::new(lines).render(area, buf);
}

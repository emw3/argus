use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::app::{ActivePane, App, DetailTab};
use crate::providers::LogLevel;

pub fn render_list(app: &App, area: Rect, buf: &mut Buffer) {
    if app.port_search_active {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        render_search_bar(app, chunks[0], buf);
        render_port_list(app, chunks[1], buf);
    } else {
        render_port_list(app, area, buf);
    }
}

fn render_search_bar(app: &App, area: Rect, buf: &mut Buffer) {
    let line = Line::from(vec![
        Span::styled(
            " /",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.port_search_query.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled("█", Style::default().fg(Color::Cyan)),
    ]);
    Paragraph::new(line).render(area, buf);
}

fn render_port_list(app: &App, area: Rect, buf: &mut Buffer) {
    let is_active = app.active_pane == ActivePane::ServiceList;
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let ports = app.filtered_ports();
    let title = format!(" Ports ({}) ", ports.len());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    if ports.is_empty() {
        Paragraph::new(Span::styled(
            "  No listening ports found",
            Style::default().fg(Color::DarkGray),
        ))
        .render(inner, buf);
        return;
    }

    let items: Vec<ListItem> = ports
        .iter()
        .enumerate()
        .map(|(i, port)| {
            let is_selected = i == app.port_selected;

            let name_style = if is_selected && is_active {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(40, 40, 60))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let port_str = format!(":{:<6}", port.port);
            let process = if port.process_name.len() > 20 {
                format!("{:.20}", port.process_name)
            } else {
                format!("{:<20}", port.process_name)
            };

            let line = Line::from(vec![
                Span::styled("● ", Style::default().fg(Color::Green)),
                Span::styled(port_str, name_style.add_modifier(Modifier::BOLD)),
                Span::styled(process, name_style),
                Span::styled(
                    format!(" {}", port.bind_address),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    let mut state = ListState::default();
    if !ports.is_empty() {
        state.select(Some(app.port_selected.min(ports.len() - 1)));
    }

    StatefulWidget::render(list, inner, buf, &mut state);
}

pub fn render_detail(app: &App, area: Rect, buf: &mut Buffer) {
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
            " Port Detail ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    render_tab_bar(app, chunks[0], buf);

    match app.detail_tab {
        DetailTab::Info => render_info(app, chunks[1], buf),
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

fn render_info(app: &App, area: Rect, buf: &mut Buffer) {
    let label_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let value_style = Style::default().fg(Color::White);

    let lines = if let Some(port) = app.selected_port_info() {
        let pid_str = port
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "N/A".to_string());

        let mut lines = vec![
            Line::from(vec![
                Span::styled("  Port    : ", label_style),
                Span::styled(
                    format!(":{}", port.port),
                    value_style.add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Bind    : ", label_style),
                Span::styled(port.bind_address.clone(), value_style),
            ]),
            Line::from(vec![
                Span::styled("  PID     : ", label_style),
                Span::styled(pid_str, value_style),
            ]),
            Line::from(vec![
                Span::styled("  Process : ", label_style),
                Span::styled(port.process_name.clone(), value_style),
            ]),
        ];

        if !port.cmdline.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Command : ", label_style),
                Span::styled(
                    port.cmdline.clone(),
                    value_style.fg(Color::Rgb(150, 150, 170)),
                ),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press 'o' to open in browser",
            Style::default().fg(Color::DarkGray),
        )));

        lines
    } else {
        vec![Line::from(Span::styled(
            "  No port selected",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    Paragraph::new(lines).render(area, buf);
}

fn render_logs(app: &App, area: Rect, buf: &mut Buffer) {
    if app.logs.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                "  No logs available",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Logs available for systemd/docker/pm2 managed processes",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        Paragraph::new(lines).render(area, buf);
        return;
    }

    let visible_height = area.height as usize;
    let total = app.logs.len();
    let start = if app.log_auto_scroll {
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

    let auto_str = if app.log_auto_scroll {
        " [AUTO] "
    } else {
        " [PAUSED] "
    };
    let scroll_hint = Line::from(Span::styled(
        format!("{}(Space to toggle)", auto_str),
        Style::default().fg(Color::DarkGray),
    ));

    if area.height > 1 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        Widget::render(List::new(items), chunks[0], buf);
        Paragraph::new(scroll_hint).render(chunks[1], buf);
    } else {
        Widget::render(List::new(items), area, buf);
    }
}

fn render_actions(area: Rect, buf: &mut Buffer) {
    let label_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);

    let lines = vec![
        Line::from(vec![
            Span::styled("  o ", label_style),
            Span::styled("Open port in browser", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  x ", label_style),
            Span::styled("Kill port process", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  / ", label_style),
            Span::styled("Search ports", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  i ", label_style),
            Span::styled("Info tab", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  l ", label_style),
            Span::styled("Logs tab", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  S-Tab ", label_style),
            Span::styled("Switch pane", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Tab ", label_style),
            Span::styled("Next view", desc_style),
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

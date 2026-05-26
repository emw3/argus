use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::app::App;
use crate::sessions::SessionInfo;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let session_name = app.preview_session.as_deref().unwrap_or("?");
    let (win, pane) = app.preview_pane;

    // Find pane info for the header
    let pane_info = app
        .sessions
        .iter()
        .filter_map(|s| {
            if let SessionInfo::Tmux(t) = s {
                Some(t)
            } else {
                None
            }
        })
        .find(|t| t.name == session_name)
        .and_then(|t| {
            t.panes
                .iter()
                .find(|p| p.window_index == win && p.pane_index == pane)
        });

    let pane_count = app
        .sessions
        .iter()
        .filter_map(|s| {
            if let SessionInfo::Tmux(t) = s {
                Some(t)
            } else {
                None
            }
        })
        .find(|t| t.name == session_name)
        .map(|t| t.panes.len())
        .unwrap_or(0);

    let cmd = pane_info.map(|p| p.command.as_str()).unwrap_or("?");
    let size = pane_info.map(|p| p.size.as_str()).unwrap_or("?");

    // Layout: header (1) + content (fill)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Header bar
    let dim = Style::default().fg(Color::DarkGray);
    let bold = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let cyan = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let header = Line::from(vec![
        Span::styled(" tmux:", dim),
        Span::styled(session_name, cyan),
        Span::styled(format!("  pane {}.{}", win, pane), bold),
        Span::styled(format!("  [{}]", cmd), Style::default().fg(Color::Yellow)),
        Span::styled(format!("  {}", size), dim),
        Span::styled(
            format!(
                "  ({}/{})",
                current_pane_index(app, session_name, win, pane) + 1,
                pane_count
            ),
            dim,
        ),
        Span::styled("    ", dim),
        Span::styled(
            "n",
            Style::default()
                .fg(Color::Rgb(200, 200, 220))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("/", dim),
        Span::styled(
            "p",
            Style::default()
                .fg(Color::Rgb(200, 200, 220))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" pane  ", dim),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::Rgb(200, 200, 220))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" back", dim),
    ]);

    let header_bg = Style::default().bg(Color::Rgb(20, 30, 40));
    buf.set_style(chunks[0], header_bg);
    header.render(chunks[0], buf);

    // Content: the captured pane output
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(40, 50, 60)))
        .title(Span::styled(
            " Live Preview ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = content_block.inner(chunks[1]);
    content_block.render(chunks[1], buf);

    if app.is_previewing_self() {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  This pane is running argus",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Can't preview itself — use n/p to switch to another pane",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        Paragraph::new(lines).render(inner, buf);
    } else if app.preview_content.is_empty() {
        Paragraph::new(Span::styled(
            "  Capturing...",
            Style::default().fg(Color::DarkGray),
        ))
        .render(inner, buf);
    } else {
        let lines: Vec<Line> = app
            .preview_content
            .lines()
            .map(|l| Line::from(Span::raw(l.to_string())))
            .collect();
        Paragraph::new(lines).render(inner, buf);
    }
}

fn current_pane_index(app: &App, session_name: &str, win: u32, pane: u32) -> usize {
    app.sessions
        .iter()
        .filter_map(|s| {
            if let SessionInfo::Tmux(t) = s {
                Some(t)
            } else {
                None
            }
        })
        .find(|t| t.name == session_name)
        .map(|t| {
            t.panes
                .iter()
                .position(|p| p.window_index == win && p.pane_index == pane)
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

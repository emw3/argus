use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

use crate::app::App;
use crate::sessions::{ClaudeMode, ConnectionType, SessionInfo};

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Sessions ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    if app.sessions.is_empty() {
        Paragraph::new(Span::styled(
            "  No active sessions detected",
            Style::default().fg(Color::DarkGray),
        ))
        .render(inner, buf);
        return;
    }

    let ssh_sessions: Vec<_> = app
        .sessions
        .iter()
        .filter_map(|s| {
            if let SessionInfo::Ssh(ssh) = s {
                Some(ssh)
            } else {
                None
            }
        })
        .collect();

    let mosh_sessions: Vec<_> = app
        .sessions
        .iter()
        .filter_map(|s| {
            if let SessionInfo::Mosh(m) = s {
                Some(m)
            } else {
                None
            }
        })
        .collect();

    let tmux_sessions: Vec<_> = app
        .sessions
        .iter()
        .filter_map(|s| {
            if let SessionInfo::Tmux(t) = s {
                Some(t)
            } else {
                None
            }
        })
        .collect();

    let claude_sessions: Vec<_> = app
        .sessions
        .iter()
        .filter_map(|s| {
            if let SessionInfo::Claude(c) = s {
                Some(c)
            } else {
                None
            }
        })
        .collect();

    let section = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let white = Style::default().fg(Color::White);

    let mut items: Vec<ListItem> = Vec::new();

    // SSH connections (excluding mosh — those show in their own section)
    let real_ssh: Vec<_> = ssh_sessions
        .iter()
        .filter(|s| s.connection_type == ConnectionType::Ssh)
        .collect();
    items.push(ListItem::new(Line::from(vec![
        Span::styled("  SSH ", section),
        Span::styled(
            format!("({})", real_ssh.len()),
            Style::default().fg(Color::Green),
        ),
    ])));
    if real_ssh.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "    No SSH connections",
            dim,
        ))));
    } else {
        for ssh in &real_ssh {
            let proc_info = if ssh.current_process.is_empty() {
                String::new()
            } else {
                format!("  running: {}", ssh.current_process)
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled("    ● ", Style::default().fg(Color::Green)),
                Span::styled(
                    format!("{}@{}", ssh.user, ssh.terminal),
                    white.add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {}", ssh.remote_ip), dim),
                Span::styled(proc_info, Style::default().fg(Color::Cyan)),
            ])));
        }
    }
    items.push(ListItem::new(Line::from("")));

    // Mosh connections
    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Mosh ", section),
        Span::styled(
            format!("({})", mosh_sessions.len()),
            Style::default().fg(Color::Cyan),
        ),
    ])));
    if mosh_sessions.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "    No mosh connections",
            dim,
        ))));
    } else {
        for ms in &mosh_sessions {
            let target = ms.tmux_target.as_deref().unwrap_or("—");
            items.push(ListItem::new(Line::from(vec![
                Span::styled("    ● ", Style::default().fg(Color::Cyan)),
                Span::styled(format!("pid {}", ms.pid), white),
                Span::styled("  → tmux:", dim),
                Span::styled(
                    target,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ])));
        }
    }
    items.push(ListItem::new(Line::from("")));

    // tmux sessions
    items.push(ListItem::new(Line::from(vec![
        Span::styled("  tmux ", section),
        Span::styled(
            format!("({})", tmux_sessions.len()),
            Style::default().fg(Color::Magenta),
        ),
    ])));
    if tmux_sessions.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "    No tmux sessions",
            dim,
        ))));
    } else {
        for (idx, ts) in tmux_sessions.iter().enumerate() {
            let is_selected = idx == app.session_cursor;
            let status_color = if ts.attached {
                Color::Green
            } else {
                Color::DarkGray
            };

            let clients_desc = if ts.attached {
                let direct = ts.attached_count.saturating_sub(ts.mosh_clients);
                let mut parts = Vec::new();
                if ts.mosh_clients > 0 {
                    parts.push(format!("{} mosh", ts.mosh_clients));
                }
                if direct > 0 {
                    parts.push(format!("{} direct", direct));
                }
                format!("  attached ({}) · {} win", parts.join(" + "), ts.windows)
            } else {
                format!("  detached · {} win", ts.windows)
            };

            let marker = if is_selected { "  ▶ " } else { "    " };
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                white.add_modifier(Modifier::BOLD)
            };

            let pane_info = if !ts.panes.is_empty() {
                format!(
                    "  {} pane{}",
                    ts.panes.len(),
                    if ts.panes.len() == 1 { "" } else { "s" }
                )
            } else {
                String::new()
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Cyan)),
                Span::styled("● ", Style::default().fg(Color::Magenta)),
                Span::styled(&ts.name, name_style),
                Span::styled(clients_desc, Style::default().fg(status_color)),
                Span::styled(pane_info, Style::default().fg(Color::DarkGray)),
                Span::styled(format!("  {}", ts.created), dim),
            ])));

            // Show pane details when selected
            if is_selected && !ts.panes.is_empty() {
                for p in &ts.panes {
                    let active_marker = if p.active { "▪" } else { " " };
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("      {} ", active_marker),
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::styled(
                            format!("{}.{}", p.window_index, p.pane_index),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(format!("  {}", p.command), Style::default().fg(Color::Cyan)),
                        Span::styled(format!("  {}", p.size), dim),
                    ])));
                }
            }
        }
    }
    items.push(ListItem::new(Line::from("")));

    // Claude Code sessions
    let agent_count = claude_sessions
        .iter()
        .filter(|c| c.mode == ClaudeMode::Agent)
        .count();
    let interactive_count = claude_sessions.len() - agent_count;
    items.push(ListItem::new(Line::from(vec![
        Span::styled("  Claude Code ", section),
        Span::styled(
            format!("({}", claude_sessions.len()),
            Style::default().fg(Color::Blue),
        ),
        Span::styled(
            format!(
                " · {} agent · {} interactive)",
                agent_count, interactive_count
            ),
            dim,
        ),
    ])));
    if claude_sessions.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "    No Claude sessions",
            dim,
        ))));
    } else {
        for cs in &claude_sessions {
            let mode_label = match cs.mode {
                ClaudeMode::Agent => ("agent", Color::Yellow),
                ClaudeMode::Interactive => ("interactive", Color::Blue),
            };
            let pid_str = cs.pid.map(|p| format!("  pid {}", p)).unwrap_or_default();
            items.push(ListItem::new(Line::from(vec![
                Span::styled("    ● ", Style::default().fg(mode_label.1)),
                Span::styled(&cs.project, white.add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("  [{}]", mode_label.0),
                    Style::default().fg(mode_label.1),
                ),
                Span::styled(pid_str, dim),
            ])));
        }
    }

    // Local terminal sessions
    let local_sessions: Vec<_> = ssh_sessions
        .iter()
        .filter(|s| s.connection_type == ConnectionType::Local)
        .collect();
    if !local_sessions.is_empty() {
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(vec![
            Span::styled("  Local Terminals ", section),
            Span::styled(format!("({})", local_sessions.len()), dim),
        ])));
        for s in &local_sessions {
            let proc_info = if s.current_process.is_empty() {
                String::new()
            } else {
                format!("  running: {}", s.current_process)
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled("    ● ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}@{}", s.user, s.terminal), white),
                Span::styled(proc_info, Style::default().fg(Color::Cyan)),
                Span::styled(format!("  {}", s.login_time), dim),
            ])));
        }
    }

    List::new(items).render(inner, buf);
}

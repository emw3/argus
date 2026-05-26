pub mod confirm;
pub mod detail;
pub mod log_view;
pub mod metrics_strip;
pub mod palette;
pub mod ports_view;
pub mod preview;
pub mod service_list;
pub mod sessions_view;
pub mod status_bar;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
    Frame,
};

use crate::app::{App, Overlay, ViewMode};
use metrics_strip::MetricsStrip;

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let has_notification = app.notification.is_some();

    // Root layout: metrics strip (1) | main (fill) | notification (0 or 1) | status bar (1)
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if has_notification {
            vec![
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(1),
            ]
        } else {
            vec![
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(0),
                Constraint::Length(1),
            ]
        })
        .split(area);

    // Render metrics strip
    frame.render_widget(MetricsStrip { app }, root_chunks[0]);

    match app.view_mode {
        ViewMode::Services => {
            let main_area = root_chunks[1];
            let is_narrow = main_area.width < 100;

            if is_narrow {
                // Vertical stack for narrow/portrait monitors
                let main_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .split(main_area);

                frame.render_widget(ServiceListWidget { app }, main_chunks[0]);
                frame.render_widget(DetailWidget { app }, main_chunks[1]);
            } else {
                // Side-by-side for wide monitors
                let main_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                    .split(main_area);

                frame.render_widget(ServiceListWidget { app }, main_chunks[0]);
                frame.render_widget(DetailWidget { app }, main_chunks[1]);
            }
        }
        ViewMode::Ports => {
            let main_area = root_chunks[1];
            let is_narrow = main_area.width < 100;

            if is_narrow {
                let main_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .split(main_area);

                frame.render_widget(PortsListWidget { app }, main_chunks[0]);
                frame.render_widget(PortsDetailWidget { app }, main_chunks[1]);
            } else {
                let main_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                    .split(main_area);

                frame.render_widget(PortsListWidget { app }, main_chunks[0]);
                frame.render_widget(PortsDetailWidget { app }, main_chunks[1]);
            }
        }
        ViewMode::Sessions => {
            frame.render_widget(SessionsViewWidget { app }, root_chunks[1]);
        }
        ViewMode::TmuxPreview => {
            frame.render_widget(PreviewWidget { app }, root_chunks[1]);
        }
    }

    // Notification bar
    if let Some(ref notif) = app.notification {
        let (icon, color) = if notif.is_error {
            (" ✗ ", Color::Red)
        } else {
            (" ✓ ", Color::Green)
        };
        let line = Line::from(vec![
            Span::styled(
                icon,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(&notif.message, Style::default().fg(Color::White)),
        ]);
        let bg = if notif.is_error {
            Color::Rgb(60, 20, 20)
        } else {
            Color::Rgb(20, 50, 20)
        };
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(bg)),
            root_chunks[2],
        );
    }

    // Status bar
    frame.render_widget(StatusBarWidget { app }, root_chunks[3]);

    // Overlays
    match app.overlay {
        Overlay::None => {}
        Overlay::Help => render_help_overlay(frame, area),
        Overlay::Palette => {
            frame.render_widget(PaletteWidget { app }, area);
        }
        Overlay::Confirm => {
            frame.render_widget(ConfirmWidget { app }, area);
        }
    }
}

// ---------------------------------------------------------------------------
// Widget wrappers (thin shims so we can pass render fns to frame.render_widget)
// ---------------------------------------------------------------------------

struct ServiceListWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for ServiceListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        service_list::render(self.app, area, buf);
    }
}

struct DetailWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for DetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        detail::render(self.app, area, buf);
    }
}

struct StatusBarWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for StatusBarWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        status_bar::render(self.app, area, buf);
    }
}

struct SessionsViewWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for SessionsViewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        sessions_view::render(self.app, area, buf);
    }
}

struct PreviewWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for PreviewWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        preview::render(self.app, area, buf);
    }
}

struct PortsListWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for PortsListWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        ports_view::render_list(self.app, area, buf);
    }
}

struct PortsDetailWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for PortsDetailWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        ports_view::render_detail(self.app, area, buf);
    }
}

struct PaletteWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for PaletteWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        palette::render(self.app, area, buf);
    }
}

struct ConfirmWidget<'a> {
    app: &'a App,
}

impl<'a> Widget for ConfirmWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        confirm::render(self.app, area, buf);
    }
}

// ---------------------------------------------------------------------------
// Help overlay
// ---------------------------------------------------------------------------

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(60, 70, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Rgb(15, 15, 25)));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let section_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let lines = vec![
        Line::from(Span::styled("  Views", section_style)),
        Line::from(vec![
            Span::styled("  Tab         ", key_style),
            Span::styled("Cycle Services → Sessions → Ports", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  S-Tab       ", key_style),
            Span::styled("Switch pane (List ↔ Detail)", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Filters", section_style)),
        Line::from(vec![
            Span::styled("  1           ", key_style),
            Span::styled("All services", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  2           ", key_style),
            Span::styled("Systemd only", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  3           ", key_style),
            Span::styled("Docker only", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  4           ", key_style),
            Span::styled("PM2 only", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Navigation", section_style)),
        Line::from(vec![
            Span::styled("  j / ↓       ", key_style),
            Span::styled("Move down", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  k / ↑       ", key_style),
            Span::styled("Move up", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  g / G       ", key_style),
            Span::styled("First / Last item", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Detail", section_style)),
        Line::from(vec![
            Span::styled("  i           ", key_style),
            Span::styled("Info tab", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  l           ", key_style),
            Span::styled("Logs tab", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Ports", section_style)),
        Line::from(vec![
            Span::styled("  o           ", key_style),
            Span::styled("Open port in browser", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Actions", section_style)),
        Line::from(vec![
            Span::styled("  s / S       ", key_style),
            Span::styled("Stop / Start service", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  r           ", key_style),
            Span::styled("Restart service", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  /           ", key_style),
            Span::styled("Search services", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  ^P          ", key_style),
            Span::styled("Command palette", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  q           ", key_style),
            Span::styled("Quit", desc_style),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

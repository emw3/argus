use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::app::App;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let running = app.running_count();
    let stopped = app.stopped_count();

    let key_style = Style::default()
        .fg(Color::Rgb(200, 200, 220))
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Rgb(100, 100, 120));
    let running_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let stopped_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);

    use crate::app::ViewMode;

    let spans = match app.view_mode {
        ViewMode::Services => vec![
            Span::styled(" Tab", key_style),
            Span::styled(" sessions ", desc_style),
            Span::styled(" 1-4", key_style),
            Span::styled(" filter ", desc_style),
            Span::styled(" /", key_style),
            Span::styled(" search ", desc_style),
            Span::styled(" ^P", key_style),
            Span::styled(" cmd ", desc_style),
            Span::styled(" ?", key_style),
            Span::styled(" help ", desc_style),
            Span::styled(" q", key_style),
            Span::styled(" quit ", desc_style),
            Span::styled("  ", Style::default()),
            Span::styled(format!("●{}", running), running_style),
            Span::styled(" ", Style::default()),
            Span::styled(format!("○{}", stopped), stopped_style),
        ],
        ViewMode::Sessions => vec![
            Span::styled(" ↑↓", key_style),
            Span::styled(" select ", desc_style),
            Span::styled(" Enter", key_style),
            Span::styled(" preview ", desc_style),
            Span::styled(" Tab", key_style),
            Span::styled(" services ", desc_style),
            Span::styled(" ?", key_style),
            Span::styled(" help ", desc_style),
            Span::styled(" q", key_style),
            Span::styled(" quit ", desc_style),
        ],
        ViewMode::Ports => vec![
            Span::styled(" ↑↓", key_style),
            Span::styled(" select ", desc_style),
            Span::styled(" o", key_style),
            Span::styled(" open ", desc_style),
            Span::styled(" i", key_style),
            Span::styled(" info ", desc_style),
            Span::styled(" l", key_style),
            Span::styled(" logs ", desc_style),
            Span::styled(" /", key_style),
            Span::styled(" search ", desc_style),
            Span::styled(" Tab", key_style),
            Span::styled(" services ", desc_style),
            Span::styled(" q", key_style),
            Span::styled(" quit ", desc_style),
        ],
        ViewMode::TmuxPreview => vec![
            Span::styled(" n", key_style),
            Span::styled("/", desc_style),
            Span::styled("p", key_style),
            Span::styled(" switch pane ", desc_style),
            Span::styled(" Esc", key_style),
            Span::styled(" back ", desc_style),
        ],
    };

    let bg_style = Style::default().bg(Color::Rgb(20, 20, 30));
    buf.set_style(area, bg_style);
    Line::from(spans).render(area, buf);
}

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::app::{App, ServiceAction};

use super::centered_rect;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let (verb, service_name, border_color) = if let Some(pa) = &app.pending_action {
        let verb = match pa.action {
            ServiceAction::Stop => "Stop",
            ServiceAction::Restart => "Restart",
            ServiceAction::Start => "Start",
            ServiceAction::Enable => "Enable",
            ServiceAction::Disable => "Disable",
        };
        let color = match pa.action {
            ServiceAction::Stop => Color::Red,
            ServiceAction::Restart => Color::Yellow,
            ServiceAction::Start => Color::Green,
            ServiceAction::Enable => Color::Green,
            ServiceAction::Disable => Color::Yellow,
        };
        (verb, pa.service_name.clone(), color)
    } else {
        // Fallback: should not normally happen when overlay==Confirm
        let services = app.filtered_services();
        let name = if services.is_empty() {
            "this service".to_string()
        } else {
            services[app.selected_index.min(services.len() - 1)]
                .name
                .clone()
        };
        ("Confirm action on", name, Color::Yellow)
    };

    let popup_area = centered_rect(50, 20, area);

    Clear.render(popup_area, buf);

    let block = Block::default()
        .title(Span::styled(
            " Confirm ",
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(Color::Rgb(15, 15, 25)));

    let inner = block.inner(popup_area);
    block.render(popup_area, buf);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw(format!("  {} ", verb)),
            Span::styled(
                &service_name,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [Enter] ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Confirm    "),
            Span::styled(
                "[Esc] ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cancel"),
        ]),
    ];

    Paragraph::new(lines).render(inner, buf);
}

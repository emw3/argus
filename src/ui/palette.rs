use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, StatefulWidget, Widget,
    },
};

use super::centered_rect;
use crate::app::App;

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    let popup_area = centered_rect(60, 60, area);

    Clear.render(popup_area, buf);

    let block = Block::default()
        .title(Span::styled(
            " Command Palette ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Rgb(15, 15, 25)));

    let inner = block.inner(popup_area);
    block.render(popup_area, buf);

    // Split: input line (1) + results list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);

    // Input line
    let input_line = Line::from(vec![
        Span::styled(
            " > ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(app.palette_query.clone(), Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(Color::Cyan)),
    ]);
    Paragraph::new(input_line)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::Rgb(40, 40, 80))),
        )
        .render(chunks[0], buf);

    // Results list
    let commands = app.palette_commands();
    let key_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let selected_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let items: Vec<ListItem> = commands
        .iter()
        .enumerate()
        .map(|(i, (cmd, desc))| {
            let is_selected = i == app.palette_selected;
            let (cmd_s, desc_s) = if is_selected {
                (selected_style, selected_style)
            } else {
                (key_style, desc_style)
            };
            let line = Line::from(vec![
                Span::styled(format!("  {:20}", cmd), cmd_s),
                Span::styled(*desc, desc_s),
            ]);
            ListItem::new(line)
        })
        .collect();

    if items.is_empty() {
        Paragraph::new(Span::styled(
            "  No matching commands",
            Style::default().fg(Color::DarkGray),
        ))
        .render(chunks[1], buf);
    } else {
        let mut state = ListState::default();
        if !commands.is_empty() {
            state.select(Some(app.palette_selected.min(commands.len() - 1)));
        }
        StatefulWidget::render(List::new(items), chunks[1], buf, &mut state);
    }
}

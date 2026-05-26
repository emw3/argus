use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::app::{ActivePane, App, ServiceFilter};
use crate::providers::{ProviderType, ServiceStatus};

fn status_dot(status: ServiceStatus) -> (&'static str, Color) {
    match status {
        ServiceStatus::Running => ("●", Color::Green),
        ServiceStatus::Stopped => ("○", Color::Red),
        ServiceStatus::Degraded => ("◉", Color::Yellow),
        ServiceStatus::Unknown => ("?", Color::DarkGray),
    }
}

fn provider_badge(provider: ProviderType) -> (&'static str, Color) {
    match provider {
        ProviderType::Systemd => ("SYS", Color::Blue),
        ProviderType::Docker => ("DOC", Color::Cyan),
        ProviderType::Pm2 => ("PM2", Color::Magenta),
    }
}

pub fn render(app: &App, area: Rect, buf: &mut Buffer) {
    if app.search_active {
        // Split into tab bar + search bar + list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(area);

        render_tabs(app, chunks[0], buf);
        render_search_bar(app, chunks[1], buf);
        render_list(app, chunks[2], buf);
    } else {
        // Split into tab bar + list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        render_tabs(app, chunks[0], buf);
        render_list(app, chunks[1], buf);
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
        Span::styled(app.search_query.clone(), Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(Color::Cyan)),
    ]);
    Paragraph::new(line).render(area, buf);
}

fn render_tabs(app: &App, area: Rect, buf: &mut Buffer) {
    let tabs = [
        (ServiceFilter::All, "1:All"),
        (ServiceFilter::Systemd, "2:Sys"),
        (ServiceFilter::Docker, "3:Doc"),
        (ServiceFilter::Pm2, "4:PM2"),
    ];

    let active_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(Color::DarkGray);

    let spans: Vec<Span> = tabs
        .iter()
        .flat_map(|(filter, label)| {
            let style = if *filter == app.service_filter {
                active_style
            } else {
                inactive_style
            };
            vec![Span::styled(format!(" {} ", label), style), Span::raw(" ")]
        })
        .collect();

    Line::from(spans).render(area, buf);
}

fn render_list(app: &App, area: Rect, buf: &mut Buffer) {
    let is_active = app.active_pane == ActivePane::ServiceList;
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            " Services ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    let services = app.filtered_services();
    let items: Vec<ListItem> = services
        .iter()
        .enumerate()
        .map(|(i, svc)| {
            let (dot, dot_color) = status_dot(svc.status);
            let (badge, badge_color) = provider_badge(svc.provider);

            let is_selected = i == app.selected_index;
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

            let line = Line::from(vec![
                Span::styled(format!("{} ", dot), Style::default().fg(dot_color)),
                Span::styled(format!("[{}] ", badge), Style::default().fg(badge_color)),
                Span::styled(svc.name.clone(), name_style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    let mut state = ListState::default();
    if !services.is_empty() {
        state.select(Some(app.selected_index.min(services.len() - 1)));
    }

    StatefulWidget::render(list, inner, buf, &mut state);
}

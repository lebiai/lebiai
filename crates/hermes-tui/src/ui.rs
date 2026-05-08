//! Rendering.

use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, Entry, Streaming};

pub fn render(frame: &mut Frame, app: &App) {
    let [header, body, footer, input] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    render_header(frame, header, app);
    render_body(frame, body, app);
    render_footer(frame, footer, app);
    render_input(frame, input, app);
}

fn render_header(frame: &mut Frame, area: ratatui::prelude::Rect, app: &App) {
    let stream_tag = match app.streaming {
        Streaming::Idle => "",
        Streaming::Thinking => " · thinking…",
        Streaming::Text => " · streaming",
    };
    let header = Line::from(vec![
        Span::styled(
            " hermes-tui ",
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}{stream_tag}", app.status.model)),
    ]);
    frame.render_widget(header, area);
}

fn render_body(frame: &mut Frame, area: ratatui::prelude::Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    for entry in &app.entries {
        match entry {
            Entry::User(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "❯ ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text.clone(), Style::default().fg(Color::Cyan)),
                ]));
                lines.push(Line::from(""));
            }
            Entry::Assistant(text) => {
                for (i, segment) in text.split('\n').enumerate() {
                    if i == 0 {
                        lines.push(Line::from(segment.to_string()));
                    } else {
                        lines.push(Line::from(segment.to_string()));
                    }
                }
                lines.push(Line::from(""));
            }
            Entry::Tool(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "⚙ ",
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM),
                    ),
                    Span::styled(text.clone(), Style::default().fg(Color::Yellow)),
                ]));
            }
            Entry::Info(text) => {
                lines.push(Line::from(Span::styled(
                    text.clone(),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            Entry::Thinking => {
                lines.push(Line::from(Span::styled(
                    "  (thinking…)",
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                )));
            }
        }
    }

    let text_len = lines.len() as u16;
    let visible_rows = area.height;
    // Pin to bottom by default; the `scroll` state offsets *upwards* from bottom.
    let max_scroll_top = text_len.saturating_sub(visible_rows);
    let scroll_top = max_scroll_top.saturating_sub(app.scroll);

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_top, 0));
    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame, area: ratatui::prelude::Rect, app: &App) {
    let right = format!(
        " mem {} ({} pinned) · skills {} · tools {} · tokens in/out {}/{}",
        app.status.memories,
        app.status.pinned,
        app.status.skills,
        app.status.tools,
        app.status.input_tokens,
        app.status.output_tokens,
    );
    let line = Line::from(vec![Span::styled(
        right,
        Style::default().fg(Color::DarkGray),
    )]);
    frame.render_widget(line, area);
}

fn render_input(frame: &mut Frame, area: ratatui::prelude::Rect, app: &App) {
    let border_style = match app.streaming {
        Streaming::Idle => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Yellow),
    };
    let hint = if app.streaming != Streaming::Idle {
        " (streaming — ^C cancel, ^D quit) "
    } else {
        " /help · ^D quit "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(hint);

    let paragraph = Paragraph::new(app.input.as_str()).block(block);
    frame.render_widget(paragraph, area);

    // Place the terminal cursor inside the input box.
    let inner_x = area.x + 1 + display_width(&app.input[..app.cursor]);
    let inner_y = area.y + 1;
    frame.set_cursor_position((inner_x, inner_y));
}

/// Printable-cell width of a string. Rough approximation: ASCII = 1, wide
/// CJK chars = 2, everything else = 1.
fn display_width(s: &str) -> u16 {
    let mut w: u16 = 0;
    for c in s.chars() {
        w = w.saturating_add(if is_wide(c) { 2 } else { 1 });
    }
    w
}

fn is_wide(c: char) -> bool {
    // Fast-path common CJK / fullwidth blocks. Not a full Unicode East Asian
    // Width table but catches the usual cases.
    let c = c as u32;
    (0x1100..=0x115F).contains(&c)
        || (0x2E80..=0x303E).contains(&c)
        || (0x3041..=0x33FF).contains(&c)
        || (0x3400..=0x4DBF).contains(&c)
        || (0x4E00..=0x9FFF).contains(&c)
        || (0xA000..=0xA4CF).contains(&c)
        || (0xAC00..=0xD7A3).contains(&c)
        || (0xF900..=0xFAFF).contains(&c)
        || (0xFE30..=0xFE4F).contains(&c)
        || (0xFF00..=0xFF60).contains(&c)
        || (0xFFE0..=0xFFE6).contains(&c)
}

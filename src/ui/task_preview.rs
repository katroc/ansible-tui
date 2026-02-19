use std::collections::HashSet;

use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::theme as th;

use super::common::*;
use super::HINTS_TASK_PREVIEW;

pub(super) fn render_task_preview(frame: &mut Frame, app: &App) {
    let theme = th::current();
    let area = centered_rect(84, 82, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(themed_modal("Task Preview"), area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(inner);

    let playbook = app
        .task_preview_playbook
        .as_deref()
        .unwrap_or("(no playbook)");
    let state = if app.task_preview_loading {
        Span::styled(
            "loading",
            theme.status_warning().add_modifier(Modifier::BOLD),
        )
    } else if app.task_preview_error.is_some() {
        Span::styled("error", theme.status_error().add_modifier(Modifier::BOLD))
    } else {
        Span::styled("ready", theme.status_success().add_modifier(Modifier::BOLD))
    };
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Playbook: ", theme.text_muted()),
            Span::styled(
                playbook.to_string(),
                theme.text().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![Span::styled("State: ", theme.text_dim()), state]),
    ]);
    frame.render_widget(header, chunks[0]);

    let lines = preview_lines(app);
    let content_rows = chunks[1].height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(content_rows.max(1));
    let scroll = app
        .task_preview_scroll
        .min(max_scroll)
        .min(u16::MAX as usize) as u16;
    let body = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .title("Plays / Tasks"),
        )
        .style(Style::default().fg(theme.fg))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(body, chunks[1]);

    let (task_count, unique_tags) = preview_summary(app);
    let mut footer_spans = hint_spans_from_bindings(&HINTS_TASK_PREVIEW, 3);
    footer_spans.push(Span::styled("  ·  ", theme.hint_sep()));
    footer_spans.push(Span::styled(
        format!("Tasks: {task_count}  Tags: {unique_tags}"),
        theme.hint_desc(),
    ));
    let footer = Paragraph::new(Line::from(footer_spans)).style(hint_bar_style());
    frame.render_widget(footer, chunks[2]);
}

fn preview_lines(app: &App) -> Vec<Line<'static>> {
    let theme = th::current();
    let mut lines = Vec::new();

    if app.task_preview_loading {
        lines.push(Line::from(Span::styled(
            "Loading playbook task graph...",
            theme.status_info().add_modifier(Modifier::BOLD),
        )));
    }

    if let Some(error) = app.task_preview_error.as_deref() {
        lines.push(Line::from(Span::styled(
            format!("Error: {error}"),
            theme.status_error(),
        )));
        lines.push(Line::raw(""));
    }

    if !app.task_preview_loading && app.task_preview_plays.is_empty() {
        lines.push(Line::from(Span::styled(
            "No plays/tasks reported by ansible-playbook --list-tasks.",
            theme.text_dim(),
        )));
    }

    for play in &app.task_preview_plays {
        lines.push(Line::from(vec![
            Span::styled("▸ ", theme.text_emphasis()),
            Span::styled(
                format!("Play: {} ({})", play.name, play.host_pattern),
                theme.text_emphasis(),
            ),
        ]));

        for (idx, task) in play.tasks.iter().enumerate() {
            let connector = if idx + 1 == play.tasks.len() {
                "  └─ "
            } else {
                "  ├─ "
            };
            let mut spans = vec![Span::styled(connector, theme.text_dim())];
            if let Some(role) = task.role.as_deref() {
                spans.push(Span::styled(
                    format!("{role} : "),
                    theme.status_info().add_modifier(Modifier::BOLD),
                ));
            }
            spans.push(Span::styled(task.name.clone(), theme.text()));
            lines.push(Line::from(spans));

            if !task.tags.is_empty() || task.when.is_some() {
                let branch_prefix = if idx + 1 == play.tasks.len() {
                    "      "
                } else {
                    "  │   "
                };
                let mut meta = vec![Span::styled(branch_prefix, theme.text_dim())];
                if !task.tags.is_empty() {
                    meta.push(Span::styled(
                        format!("tags: [{}]", task.tags.join(", ")),
                        theme.text_dim(),
                    ));
                }
                if let Some(when) = task.when.as_deref() {
                    if !task.tags.is_empty() {
                        meta.push(Span::styled("  ·  ", theme.hint_sep()));
                    }
                    meta.push(Span::styled(
                        format!("when: {when}"),
                        theme.status_warning().add_modifier(Modifier::ITALIC),
                    ));
                }
                lines.push(Line::from(meta));
            }
        }

        if play.tasks.is_empty() {
            lines.push(Line::from(Span::styled("  (no tasks)", theme.text_dim())));
        }
        lines.push(Line::raw(""));
    }

    if !app.task_preview_logs.is_empty() {
        let overflow = app.task_preview_logs.len().saturating_sub(10);
        lines.push(Line::from(Span::styled(
            "Warnings / stderr:",
            theme.status_warning().add_modifier(Modifier::BOLD),
        )));
        if overflow > 0 {
            lines.push(Line::from(Span::styled(
                format!("  ... {overflow} older lines omitted ..."),
                theme.text_dim(),
            )));
        }
        for line in app.task_preview_logs.iter().skip(overflow) {
            lines.push(Line::from(vec![
                Span::styled("  • ", theme.text_dim()),
                Span::styled(line.clone(), theme.text_dim()),
            ]));
        }
    }

    lines
}

fn preview_summary(app: &App) -> (usize, usize) {
    let mut task_count = 0usize;
    let mut tags = HashSet::new();
    for play in &app.task_preview_plays {
        for task in &play.tasks {
            task_count += 1;
            for tag in &task.tags {
                tags.insert(tag.clone());
            }
        }
    }
    (task_count, tags.len())
}

use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Table,
    TableState, Wrap,
};
use ratatui::Frame;

use crate::app::{display_path, App, FilterTarget, FocusContext, View};
use crate::playbook_settings::PlaybookSettings;
use crate::theme as th;

use super::common::*;
use super::{HINTS_SETTINGS_EDITOR, HINTS_SETTINGS_EDITOR_TEXT};

pub(super) fn render_playbooks(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(14), Constraint::Length(8)])
        .split(area);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
        .split(chunks[0]);

    let filtered_playbook_indices = app.filtered_playbook_indices();
    let items = if app.playbooks.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::raw("No playbooks found. "),
            Span::styled("Add .yml files to ./playbooks", Style::default().fg(th::SUBTEXT0)),
        ]))]
    } else if filtered_playbook_indices.is_empty() {
        vec![ListItem::new("No playbooks match current filter")]
    } else {
        filtered_playbook_indices
            .iter()
            .map(|idx| {
                ListItem::new(display_path(
                    app.active_project_root(),
                    &app.playbooks[*idx],
                ))
            })
            .collect::<Vec<_>>()
    };
    let focus_ctx = app.content_focus_context();
    let playbooks_focused = matches!(focus_ctx, FocusContext::PlaybooksList);
    let runs_focused = matches!(focus_ctx, FocusContext::PlaybooksRuns);
    let playbooks_border_style = if playbooks_focused {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let runs_border_style = if runs_focused {
        Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(playbooks_border_style)
                .style(focus_bg(playbooks_focused))
                .title(filtered_list_title(
                    "Playbooks",
                    app.filter_query_for(FilterTarget::Playbooks),
                    app.is_filter_editing_target(FilterTarget::Playbooks),
                )),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut state = ListState::default().with_selected(
        filtered_playbook_indices
            .iter()
            .position(|idx| *idx == app.playbook_idx),
    );
    frame.render_stateful_widget(list, top[0], &mut state);

    let run_indices = app.run_indices_for_selected_playbook();
    let run_items = if app.selected_playbook_display().is_none() {
        vec![ListItem::new("No playbook selected")]
    } else if run_indices.is_empty() {
        if app
            .filter_query_for(FilterTarget::PlaybookRuns)
            .trim()
            .is_empty()
        {
            vec![ListItem::new(
                "No runs yet for this playbook. Press r to run.",
            )]
        } else {
            vec![ListItem::new("No runs match current filter")]
        }
    } else {
        run_indices
            .iter()
            .map(|idx| {
                let run = &app.runs[*idx];
                let code = run
                    .exit_code
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| String::from("-"));
                ListItem::new(Line::from(Span::raw(format!(
                    "#{:03} {:>9} code:{:<4} {}",
                    run.id,
                    run.status.as_str(),
                    code,
                    run.started_at.format("%H:%M:%S")
                ))))
            })
            .collect::<Vec<_>>()
    };
    let runs = List::new(run_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(runs_border_style)
                .style(focus_bg(runs_focused))
                .title(filtered_list_title(
                    &format!("Runs For Selected Playbook ({})", app.active_project_name()),
                    app.filter_query_for(FilterTarget::PlaybookRuns),
                    app.is_filter_editing_target(FilterTarget::PlaybookRuns),
                )),
        )
        .highlight_style(Style::default().fg(th::GREEN))
        .highlight_symbol(">> ");
    let selected_run_pos = run_indices.iter().position(|idx| *idx == app.run_idx);
    let mut runs_state = ListState::default().with_selected(selected_run_pos);
    frame.render_stateful_widget(runs, top[1], &mut runs_state);

    if let Some(settings) = app.selected_playbook_settings() {
        let selected = app
            .selected_playbook_display()
            .unwrap_or_else(|| String::from("(none)"));
        let selected_inventory = app
            .selected_inventory_display_for_current_playbook()
            .unwrap_or_else(|| String::from("(none)"));
        let rows = styled_key_value_rows(settings_preview_rows(
            &selected,
            &selected_inventory,
            &settings,
        ));
        let (detail_cols, detail_spacing) = key_value_table_layout(chunks[1], 22);
        let table = Table::new(rows, detail_cols)
            .column_spacing(detail_spacing)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(neutral_border_style())
                    .style(Style::default().bg(th::BASE))
                    .title("Playbook Settings"),
            );
        frame.render_widget(table, chunks[1]);
    } else {
        let paragraph = Paragraph::new(vec![
            Line::raw("No playbook selected"),
            Line::raw("Press t in Playbooks tab to create/edit settings"),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .title("Playbook Settings"),
        );
        frame.render_widget(paragraph, chunks[1]);
    }
}

pub(super) fn render_logs(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let empty_message = if app.current_view() == View::Templates {
        "No run selected for this template yet."
    } else {
        "No run selected for this playbook yet."
    };
    let content = app
        .runs
        .get(app.run_idx)
        .map(|run| {
            let viewport_height = area.height.saturating_sub(2) as usize;

            let mut lines = vec![Line::styled(
                format!(
                    "Run #{}, playbook: {}, inventory: {}, status: {}",
                    run.id,
                    run.playbook,
                    run.inventory,
                    run.status.as_str()
                ),
                Style::default()
                    .fg(th::SUBTEXT1)
                    .add_modifier(Modifier::BOLD),
            )];
            lines.push(Line::raw(""));

            let log_slots = viewport_height.saturating_sub(lines.len());
            if run.logs.is_empty() {
                lines.push(Line::styled(
                    "No logs yet for this run.",
                    Style::default().fg(th::SUBTEXT0),
                ));
            } else if log_slots > 0 {
                if app.log_select_mode {
                    let cursor = app.log_cursor.min(run.logs.len() - 1);
                    let anchor = app.log_anchor.unwrap_or(cursor).min(run.logs.len() - 1);
                    let sel_start = anchor.min(cursor);
                    let sel_end = anchor.max(cursor);

                    let mut start = cursor.saturating_sub(log_slots.saturating_sub(1));
                    if start + log_slots > run.logs.len() {
                        start = run.logs.len().saturating_sub(log_slots);
                    }
                    let end = (start + log_slots).min(run.logs.len());

                    for (idx, line) in run.logs.iter().enumerate().take(end).skip(start) {
                        let mut style = log_line_style(line);
                        if idx >= sel_start && idx <= sel_end {
                            style = style.bg(th::SURFACE1).add_modifier(Modifier::BOLD);
                        }
                        if idx == cursor {
                            style = style.bg(th::MANTLE).add_modifier(Modifier::REVERSED);
                        }
                        lines.push(Line::styled(format!("{:>4} {}", idx + 1, line), style));
                    }
                } else {
                    let start = run.logs.len().saturating_sub(log_slots);
                    for line in run.logs.iter().skip(start) {
                        lines.push(Line::styled(line.clone(), log_line_style(line)));
                    }
                }
            }
            Text::from(lines)
        })
        .unwrap_or_else(|| Text::from(empty_message));

    let focus_ctx = app.content_focus_context();
    let (title, border_style) = if app.current_view() == View::Templates {
        (
            "Template Run Logs",
            if matches!(focus_ctx, FocusContext::TemplatesLogSelect) {
                Style::default()
                    .fg(th::FOCUS_BORDER)
                    .add_modifier(Modifier::BOLD)
            } else if matches!(focus_ctx, FocusContext::TemplatesRuns) {
                Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD)
            } else {
                neutral_border_style()
            },
        )
    } else {
        (
            "Live Logs (Selected Run)",
            if matches!(focus_ctx, FocusContext::PlaybooksLogSelect) {
                Style::default()
                    .fg(th::FOCUS_BORDER)
                    .add_modifier(Modifier::BOLD)
            } else {
                neutral_border_style()
            },
        )
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(title),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

pub(super) fn render_playbook_settings_editor(frame: &mut Frame, app: &App) {
    let area = centered_rect(76, 72, frame.area());
    frame.render_widget(Clear, area);

    let border_color = if app.settings_editor_text_mode {
        th::YELLOW
    } else {
        th::MAUVE
    };
    let wrapper = Block::default()
        .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Playbook Settings");
    frame.render_widget(wrapper, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(2),
        ])
        .split(inner);

    let mode_label = if app.settings_editor_text_mode {
        "Edit mode: ON"
    } else {
        "Edit mode: OFF"
    };
    let mode_style = if app.settings_editor_text_mode {
        Style::default()
            .fg(th::CRUST)
            .bg(th::YELLOW)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th::SUBTEXT1)
    };
    frame.render_widget(Paragraph::new(mode_label).style(mode_style), chunks[0]);

    let playbook = app
        .selected_playbook_display()
        .unwrap_or_else(|| String::from("(none)"));
    let header = Paragraph::new(format!("Playbook: {playbook}"))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style()),
        )
        .style(Style::default().fg(th::TEXT).bg(th::BASE));
    frame.render_widget(header, chunks[1]);

    let settings = app.selected_playbook_settings().unwrap_or_default();
    let rows = settings_rows(app, &settings);
    let (detail_cols, detail_spacing) = key_value_table_layout(chunks[2], 30);
    let fields = Table::new(styled_key_value_rows(rows), detail_cols)
        .column_spacing(detail_spacing)
        .row_highlight_style(
            Style::default()
                .fg(th::CRUST)
                .bg(th::YELLOW)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ")
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .style(Style::default().fg(th::TEXT).bg(th::BASE))
                .title("Fields"),
        );
    let mut fields_state = TableState::default().with_selected(Some(app.settings_editor_field_idx));
    frame.render_stateful_widget(fields, chunks[2], &mut fields_state);

    let hint = Paragraph::new(if app.settings_text_mode_is_multiline() {
        hint_line_from_bindings(&HINTS_SETTINGS_EDITOR_TEXT, 6)
    } else {
        hint_line_from_bindings(&HINTS_SETTINGS_EDITOR, 6)
    })
    .style(hint_bar_style());
    frame.render_widget(hint, chunks[3]);
}

pub(super) fn settings_preview_rows(
    selected_playbook: &str,
    selected_inventory: &str,
    settings: &PlaybookSettings,
) -> Vec<(String, String)> {
    vec![
        (String::from("playbook"), selected_playbook.to_string()),
        (
            String::from("inventory target"),
            selected_inventory.to_string(),
        ),
        (String::from("check"), settings.check.to_string()),
        (String::from("diff"), settings.diff.to_string()),
        (String::from("become"), settings.become_enabled.to_string()),
        (String::from("verbosity"), settings.verbosity.to_string()),
        (
            String::from("forks"),
            settings
                .forks
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("timeout"),
            settings
                .timeout
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("limit"),
            settings
                .limit
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("tags"),
            settings
                .tags
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("extra-vars/files"),
            settings
                .extra_vars
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("additional args"),
            settings
                .extra_args
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("ssh_key_file"),
            settings
                .ssh_private_key_file
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("ssh_key_inline"),
            summarize_inline_key(settings.ssh_private_key_inline.as_deref()),
        ),
    ]
}

pub(super) fn settings_rows(app: &App, settings: &PlaybookSettings) -> Vec<(String, String)> {
    vec![
        (String::from("check (--check)"), settings.check.to_string()),
        (String::from("diff (--diff)"), settings.diff.to_string()),
        (
            String::from("become (--become)"),
            settings.become_enabled.to_string(),
        ),
        (
            String::from("verbosity (-v)"),
            settings.verbosity.to_string(),
        ),
        (
            String::from("forks (--forks)"),
            settings
                .forks
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("timeout (--timeout)"),
            settings
                .timeout
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("limit (--limit)"),
            display_setting_text(app, 6, settings.limit.as_deref().unwrap_or("unset")),
        ),
        (
            String::from("tags (--tags)"),
            display_setting_text(app, 7, settings.tags.as_deref().unwrap_or("unset")),
        ),
        (
            String::from("extra-vars/files (--extra-vars)"),
            display_setting_text(app, 8, settings.extra_vars.as_deref().unwrap_or("unset")),
        ),
        (
            String::from("additional args (appended)"),
            display_setting_text(app, 9, settings.extra_args.as_deref().unwrap_or("unset")),
        ),
        (
            String::from("ssh private key file (--private-key)"),
            display_setting_text(
                app,
                10,
                settings.ssh_private_key_file.as_deref().unwrap_or("unset"),
            ),
        ),
        (
            String::from("ssh private key inline"),
            display_setting_inline_key_text(app, 11, settings.ssh_private_key_inline.as_deref()),
        ),
    ]
}

pub(super) fn display_setting_text(app: &App, idx: usize, current: &str) -> String {
    if app.settings_editor_text_mode && app.settings_editor_field_idx == idx {
        if app.settings_editor_text_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.settings_editor_text_buffer)
        }
    } else {
        current.to_string()
    }
}

pub(super) fn display_setting_inline_key_text(app: &App, idx: usize, current: Option<&str>) -> String {
    if app.settings_editor_text_mode && app.settings_editor_field_idx == idx {
        let escaped = app
            .settings_editor_text_buffer
            .replace('\n', "\\n")
            .replace('\t', "\\t");
        if escaped.is_empty() {
            String::from("|")
        } else {
            format!("{escaped}|")
        }
    } else {
        summarize_inline_key(current)
    }
}


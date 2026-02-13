use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Table, TableState,
    Wrap,
};
use ratatui::Frame;

use crate::app::{App, View};
use crate::run::playbook_bin_available;
use crate::theme as th;

use super::common::*;
use super::HINTS_RUNTIME_PROMPT;

pub(super) fn render_settings(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let theme = th::current();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(8)])
        .split(area);

    let mode_line = if app.global_settings_text_mode {
        Paragraph::new("Global settings edit mode: ON").style(theme.chrome_accent())
    } else {
        Paragraph::new("Global settings edit mode: OFF").style(theme.text_muted())
    };
    frame.render_widget(mode_line, chunks[0]);

    let (detail_cols, detail_spacing) = key_value_table_layout(chunks[1], 30);
    let table = Table::new(
        styled_key_value_rows(global_settings_rows(app)),
        detail_cols,
    )
    .column_spacing(detail_spacing)
    .row_highlight_style(theme.table_highlight())
    .highlight_symbol(HIGHLIGHT_SYMBOL)
    .block(themed_panel(
        "Global Settings",
        matches!(
            app.content_focus_context(),
            crate::app::FocusContext::Settings
        ),
    ));
    let mut state = TableState::default().with_selected(Some(app.global_settings_field_idx));
    frame.render_stateful_widget(table, chunks[1], &mut state);
}

pub(super) fn global_settings_rows(app: &App) -> Vec<(String, String)> {
    vec![
        (
            String::from("ansible bin"),
            display_global_settings_text(app, 0, &app.run_options.ansible_bin),
        ),
        (
            String::from("interpreter_python"),
            display_global_settings_text(
                app,
                1,
                app.ansible_cfg
                    .interpreter_python
                    .as_deref()
                    .unwrap_or("auto (unset)"),
            ),
        ),
        (
            String::from("forks"),
            app.ansible_cfg
                .forks
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("timeout"),
            app.ansible_cfg
                .timeout
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("verbosity"),
            app.ansible_cfg.verbosity.to_string(),
        ),
        (
            String::from("host_key_checking"),
            app.ansible_cfg.host_key_checking.to_string(),
        ),
        (
            String::from("stdout_callback"),
            display_global_settings_text(
                app,
                6,
                app.ansible_cfg
                    .stdout_callback
                    .as_deref()
                    .unwrap_or("default (unset)"),
            ),
        ),
        (
            String::from("retry_files_enabled"),
            app.ansible_cfg.retry_files_enabled.to_string(),
        ),
        (
            String::from("retry_files_save_path"),
            display_global_settings_text(
                app,
                8,
                app.ansible_cfg
                    .retry_files_save_path
                    .as_deref()
                    .unwrap_or("unset"),
            ),
        ),
        (
            String::from("remote_user"),
            display_global_settings_text(
                app,
                9,
                app.ansible_cfg.remote_user.as_deref().unwrap_or("unset"),
            ),
        ),
        (
            String::from("private_key_file"),
            display_global_settings_text(
                app,
                10,
                app.ansible_cfg
                    .private_key_file
                    .as_deref()
                    .unwrap_or("unset"),
            ),
        ),
        (
            String::from("pipelining"),
            app.ansible_cfg.pipelining.to_string(),
        ),
        (
            String::from("secret_enforcement_mode"),
            app.secret_enforcement_mode.as_str().to_string(),
        ),
    ]
}

pub(super) fn display_global_settings_text(app: &App, idx: usize, current: &str) -> String {
    if app.current_view() == View::Settings
        && app.global_settings_text_mode
        && app.global_settings_field_idx == idx
    {
        if app.global_settings_text_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.global_settings_text_buffer)
        }
    } else {
        current.to_string()
    }
}

pub(super) fn render_runtime_prompt(frame: &mut Frame, app: &App) {
    let theme = th::current();
    let area = centered_rect(88, 78, frame.area());
    frame.render_widget(Clear, area);

    frame.render_widget(themed_modal("Runtime Setup"), area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(9),
            Constraint::Length(1),
            Constraint::Min(4),
        ])
        .split(inner);

    let runtime_required = !playbook_bin_available(&app.run_options.ansible_bin);
    let alert_style = if app.runtime_bootstrapping {
        theme.status_warning().add_modifier(Modifier::BOLD)
    } else if runtime_required {
        theme.status_error()
    } else {
        theme.status_success().add_modifier(Modifier::BOLD)
    };
    let alert_text = if app.runtime_bootstrapping {
        "Runtime bootstrap in progress. Please wait."
    } else if runtime_required {
        "Action required: no usable ansible-playbook runtime is configured."
    } else {
        "Runtime detected. You can keep this selection or choose a different runtime."
    };
    let alert = Paragraph::new(alert_text)
        .style(alert_style)
        .wrap(Wrap { trim: true });
    frame.render_widget(alert, chunks[0]);

    let title = if app.runtime_bootstrapping {
        Span::styled(" Runtime Setup (bootstrapping) ", theme.status_warning())
    } else if runtime_required {
        Span::styled(" Runtime Setup Required ", theme.status_error())
    } else {
        Span::styled(" Runtime Selector ", theme.status_success())
    };
    let header = Paragraph::new(
        "Select an existing Ansible runtime below, or press b to install a managed runtime in ./.ansible-tui/runtime.",
    )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .style(theme.modal_bg())
                .title(title),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(header, chunks[1]);

    let candidate_items = if app.runtime_candidates.is_empty() {
        vec![ListItem::new(
            "No runtime candidates found. Press b to bootstrap managed runtime.",
        )]
    } else {
        app.runtime_candidates
            .iter()
            .map(|c| {
                ListItem::new(format!(
                    "[{}] {} -> {}",
                    if c.available { "ok" } else { "missing" },
                    c.label,
                    c.ansible_bin
                ))
            })
            .collect::<Vec<_>>()
    };
    let candidates = List::new(candidate_items)
        .block(themed_panel("Candidates", true))
        .highlight_style(theme.list_highlight())
        .highlight_symbol(HIGHLIGHT_SYMBOL);
    let mut state = ListState::default().with_selected(if app.runtime_candidates.is_empty() {
        None
    } else {
        Some(app.runtime_candidate_idx)
    });
    frame.render_stateful_widget(candidates, chunks[2], &mut state);

    let hints =
        Paragraph::new(hint_line_from_bindings(&HINTS_RUNTIME_PROMPT, 6)).style(hint_bar_style());
    frame.render_widget(hints, chunks[3]);

    let runtime_logs = if app.runtime_logs.is_empty() {
        Text::from("No runtime setup logs yet.")
    } else {
        let lines = app
            .runtime_logs
            .iter()
            .rev()
            .take(8)
            .rev()
            .map(|line| Line::raw(line.clone()))
            .collect::<Vec<_>>();
        Text::from(lines)
    };
    let logs = Paragraph::new(runtime_logs)
        .block(themed_panel("Setup Logs", false))
        .wrap(Wrap { trim: false });
    frame.render_widget(logs, chunks[4]);
}

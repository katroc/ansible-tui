use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Table, TableState,
};
use ratatui::Frame;

use crate::app::{display_path, App, FilterTarget, FocusContext};
use crate::theme as th;

use super::common::*;
use super::{HINTS_TEMPLATE_EDITOR, HINTS_TEMPLATE_EDITOR_TEXT};

pub(super) fn render_templates(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let theme = th::current();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(14), Constraint::Length(8)])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
        .split(chunks[0]);

    let filtered_template_indices = app.filtered_template_indices();
    let template_items = if app.job_templates.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::raw("No templates yet. "),
            Span::styled("Press n to create one", theme.text_dim()),
        ]))]
    } else if filtered_template_indices.is_empty() {
        vec![ListItem::new("No templates match current filter")]
    } else {
        filtered_template_indices
            .iter()
            .map(|idx| {
                let template = &app.job_templates[*idx];
                ListItem::new(Line::from(vec![
                    Span::styled(
                        template.name.clone(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(format!("({})", template.playbook), theme.text_dim()),
                ]))
            })
            .collect::<Vec<_>>()
    };

    let focus_ctx = app.content_focus_context();
    let templates_focused = matches!(focus_ctx, FocusContext::TemplatesList);
    let template_list = List::new(template_items)
        .block(themed_panel(
            filtered_list_title(
                "Templates",
                app.filter_query_for(FilterTarget::Templates),
                app.is_filter_editing_target(FilterTarget::Templates),
            ),
            templates_focused,
        ))
        .highlight_style(theme.list_highlight())
        .highlight_symbol(HIGHLIGHT_SYMBOL);
    let selected_template_pos = filtered_template_indices
        .iter()
        .position(|idx| *idx == app.template_idx);
    let mut template_state = ListState::default().with_selected(selected_template_pos);
    frame.render_stateful_widget(template_list, top[0], &mut template_state);

    let template_run_indices = app.run_indices_for_selected_template();
    let template_run_items = if app.selected_template().is_none() {
        vec![ListItem::new("No template selected")]
    } else if template_run_indices.is_empty() {
        if app
            .filter_query_for(FilterTarget::TemplateRuns)
            .trim()
            .is_empty()
        {
            vec![ListItem::new(
                "No runs yet for this template. Press r to run.",
            )]
        } else {
            vec![ListItem::new("No runs match current filter")]
        }
    } else {
        template_run_indices
            .iter()
            .map(|idx| {
                let run = &app.runs[*idx];
                let code = run
                    .exit_code
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| String::from("-"));
                ListItem::new(format!(
                    "#{:03} {:>9} code:{:<4} {}",
                    run.id,
                    run.status.as_str(),
                    code,
                    run.started_at.format("%H:%M:%S")
                ))
            })
            .collect::<Vec<_>>()
    };
    let runs_focused = matches!(focus_ctx, FocusContext::TemplatesRuns);
    let run_list = List::new(template_run_items)
        .block(themed_panel(
            filtered_list_title(
                "Runs For Selected Template",
                app.filter_query_for(FilterTarget::TemplateRuns),
                app.is_filter_editing_target(FilterTarget::TemplateRuns),
            ),
            runs_focused,
        ))
        .highlight_style(theme.list_highlight())
        .highlight_symbol(HIGHLIGHT_SYMBOL);
    let selected_run_pos = template_run_indices
        .iter()
        .position(|idx| *idx == app.run_idx);
    let mut run_state = ListState::default().with_selected(selected_run_pos);
    frame.render_stateful_widget(run_list, top[1], &mut run_state);

    let rows = if let Some(template) = app.selected_template() {
        template_settings_preview_rows(app, template)
    } else {
        vec![
            (String::from("template"), String::from("none")),
            (String::from("playbook"), String::from("unset")),
            (String::from("inventory"), String::from("unset")),
            (String::from("check"), String::from("false")),
            (String::from("diff"), String::from("false")),
            (String::from("become"), String::from("false")),
            (String::from("verbosity"), String::from("0")),
            (String::from("forks"), String::from("unset")),
            (String::from("timeout"), String::from("unset")),
            (String::from("limit"), String::from("unset")),
            (String::from("tags"), String::from("unset")),
            (String::from("extra-vars"), String::from("unset")),
            (String::from("additional args"), String::from("unset")),
            (String::from("ssh_key_file"), String::from("unset")),
            (String::from("ssh_key_inline"), String::from("unset")),
        ]
    };
    let (detail_cols, detail_spacing) = key_value_table_layout(chunks[1], 22);
    let table = Table::new(styled_key_value_rows(rows), detail_cols)
        .column_spacing(detail_spacing)
        .block(themed_panel("Template Settings", false));
    frame.render_widget(table, chunks[1]);
}

pub(super) fn render_template_editor(frame: &mut Frame, app: &App) {
    let theme = th::current();
    let area = centered_rect(86, 84, frame.area());
    frame.render_widget(Clear, area);

    let title = if app.template_editor_editing_id.is_some() {
        "Template Editor (Edit)"
    } else {
        "Template Editor (New)"
    };
    let wrapper = themed_modal(title);
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
            Constraint::Min(14),
            Constraint::Length(1),
        ])
        .split(inner);

    let mode_label = if app.template_editor_text_mode {
        "Edit mode: ON"
    } else {
        "Edit mode: OFF"
    };
    let mode_style = if app.template_editor_text_mode {
        theme.chrome_accent()
    } else {
        theme.text_muted()
    };
    frame.render_widget(Paragraph::new(mode_label).style(mode_style), chunks[0]);

    let selected_playbook = app
        .playbooks
        .get(app.template_editor_playbook_idx)
        .map(|path| display_path(app.active_project_root(), path))
        .unwrap_or_else(|| String::from("(none)"));
    let selected_inventory = app
        .inventories
        .get(app.template_editor_inventory_idx)
        .map(|path| display_path(app.active_project_root(), path))
        .unwrap_or_else(|| String::from("(none)"));
    let header = Paragraph::new(format!(
        "Playbook: {selected_playbook} | Inventory: {selected_inventory} | Scope: active project"
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(neutral_border_style()),
    )
    .style(theme.modal_bg());
    frame.render_widget(header, chunks[1]);

    let rows = template_editor_rows(app);
    let (detail_cols, detail_spacing) = key_value_table_layout(chunks[2], 34);
    let fields = Table::new(styled_key_value_rows(rows), detail_cols)
        .column_spacing(detail_spacing)
        .row_highlight_style(theme.table_highlight())
        .highlight_symbol(HIGHLIGHT_SYMBOL)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .style(theme.modal_bg())
                .title("Template Fields"),
        );
    let mut fields_state = TableState::default().with_selected(Some(app.template_editor_field_idx));
    frame.render_stateful_widget(fields, chunks[2], &mut fields_state);

    let hint = Paragraph::new(
        if app.template_editor_text_mode && app.template_editor_is_multiline_field() {
            hint_line_from_bindings(&HINTS_TEMPLATE_EDITOR_TEXT, 6)
        } else {
            hint_line_from_bindings(&HINTS_TEMPLATE_EDITOR, 6)
        },
    )
    .style(hint_bar_style());
    frame.render_widget(hint, chunks[3]);
}

pub(super) fn template_editor_rows(app: &App) -> Vec<(String, String)> {
    let settings = &app.template_editor_settings;
    let selected_inventory_display = app
        .inventories
        .get(app.template_editor_inventory_idx)
        .map(|path| display_path(app.active_project_root(), path))
        .unwrap_or_else(|| String::from("(none)"));

    vec![
        (
            String::from("name"),
            display_template_editor_text(app, 0, &app.template_editor_name),
        ),
        (
            String::from("playbook"),
            app.playbooks
                .get(app.template_editor_playbook_idx)
                .map(|path| display_path(app.active_project_root(), path))
                .unwrap_or_else(|| String::from("(none)")),
        ),
        (String::from("inventory"), selected_inventory_display),
        (String::from("scope"), String::from("active project")),
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
            display_template_editor_text(app, 10, settings.limit.as_deref().unwrap_or("unset")),
        ),
        (
            String::from("tags (--tags)"),
            display_template_editor_text(app, 11, settings.tags.as_deref().unwrap_or("unset")),
        ),
        (
            String::from("extra-vars/files (--extra-vars)"),
            display_template_editor_text(
                app,
                12,
                settings.extra_vars.as_deref().unwrap_or("unset"),
            ),
        ),
        (
            String::from("additional args (appended)"),
            display_template_editor_text(
                app,
                13,
                settings.extra_args.as_deref().unwrap_or("unset"),
            ),
        ),
        (
            String::from("ssh private key file (--private-key)"),
            display_template_editor_text(
                app,
                14,
                settings.ssh_private_key_file.as_deref().unwrap_or("unset"),
            ),
        ),
        (
            String::from("ssh private key inline"),
            display_template_editor_inline_key_text(
                app,
                15,
                settings.ssh_private_key_inline.as_deref(),
            ),
        ),
        (
            String::from("vault source (prompt/file)"),
            app.template_editor_vault_source_type
                .map(|value| value.as_str().to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("vault password file"),
            display_template_editor_text(
                app,
                17,
                if app.template_editor_vault_password_file.is_empty() {
                    "unset"
                } else {
                    &app.template_editor_vault_password_file
                },
            ),
        ),
        (
            String::from("vault id label"),
            display_template_editor_text(
                app,
                18,
                if app.template_editor_vault_id_label.is_empty() {
                    "unset"
                } else {
                    &app.template_editor_vault_id_label
                },
            ),
        ),
    ]
}

pub(super) fn template_settings_preview_rows(
    app: &App,
    template: &crate::job_template::JobTemplate,
) -> Vec<(String, String)> {
    let mut rows = vec![
        (String::from("template"), template.name.clone()),
        (String::from("playbook"), template.playbook.clone()),
        (String::from("inventory"), template.inventory.clone()),
        (String::from("scope"), String::from("active project")),
        (String::from("check"), template.check.to_string()),
        (String::from("diff"), template.diff.to_string()),
        (String::from("become"), template.become_enabled.to_string()),
        (String::from("verbosity"), template.verbosity.to_string()),
        (
            String::from("forks"),
            template
                .forks
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("timeout"),
            template
                .timeout
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("limit"),
            template
                .limit
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("tags"),
            template
                .tags
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("extra-vars/files"),
            template
                .extra_vars
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("additional args"),
            template
                .extra_args
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("ssh_key_file"),
            template
                .ssh_private_key_file
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("ssh_key_inline"),
            summarize_inline_key(template.ssh_private_key_inline.as_deref()),
        ),
        (
            String::from("vault_source"),
            template
                .vault_source_type
                .map(|value| value.as_str().to_string())
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("vault_password_file"),
            template
                .vault_password_file
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
        (
            String::from("vault_id_label"),
            template
                .vault_id_label
                .clone()
                .unwrap_or_else(|| String::from("unset")),
        ),
    ];

    match app.template_effective_context(template) {
        Ok(context) => {
            rows.push((
                String::from("effective inventory"),
                format!("{} ({})", context.inventory, context.inventory_source),
            ));
            rows.push((
                String::from("effective vars files"),
                if context.vars_files.is_empty() {
                    String::from("none")
                } else {
                    context.vars_files.join(", ")
                },
            ));
            rows.push((
                String::from("effective ssh key"),
                if context.has_inline_ssh_key {
                    format!("inline ({})", context.ssh_key_source)
                } else {
                    context
                        .ssh_private_key_file
                        .clone()
                        .map(|path| format!("{path} ({})", context.ssh_key_source))
                        .unwrap_or_else(|| String::from("unset"))
                },
            ));
            rows.push((String::from("effective vault source"), context.vault_source));
            rows.push((
                String::from("effective vault id"),
                context
                    .vault_id_label
                    .clone()
                    .unwrap_or_else(|| String::from("unset")),
            ));
            if !context.warnings.is_empty() {
                rows.push((
                    String::from("context warnings"),
                    context.warnings.join(" | "),
                ));
            }
        }
        Err(err) => rows.push((String::from("context error"), err)),
    }

    rows
}

pub(super) fn display_template_editor_text(app: &App, idx: usize, current: &str) -> String {
    if app.template_editor_text_mode && app.template_editor_field_idx == idx {
        if app.template_editor_text_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.template_editor_text_buffer)
        }
    } else {
        current.to_string()
    }
}

pub(super) fn display_template_editor_inline_key_text(
    app: &App,
    idx: usize,
    current: Option<&str>,
) -> String {
    if app.template_editor_text_mode && app.template_editor_field_idx == idx {
        let escaped = app
            .template_editor_text_buffer
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

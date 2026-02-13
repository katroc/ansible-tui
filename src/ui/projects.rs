use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Table, Wrap};
use ratatui::Frame;

use crate::app::{display_path, App, FilterTarget, FocusContext, ProjectCreateMode};
use crate::theme as th;

use super::common::*;
use super::{HINTS_PROJECT_CREATE, HINTS_PROJECT_SECRETS};

pub(super) fn render_projects(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let theme = th::current();
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);

    let filtered_project_indices = app.filtered_project_indices();
    let items = if app.projects.is_empty() {
        vec![ListItem::new("No projects configured.")]
    } else if filtered_project_indices.is_empty() {
        vec![ListItem::new("No projects match current filter.")]
    } else {
        filtered_project_indices
            .iter()
            .map(|idx| {
                let project = &app.projects[*idx];
                let active = if *idx == app.active_project_idx {
                    "*"
                } else {
                    " "
                };
                ListItem::new(format!("[{active}] {}", project.name))
            })
            .collect::<Vec<_>>()
    };
    let list_focused = matches!(app.content_focus_context(), FocusContext::Projects);
    let list = List::new(items)
        .block(themed_panel(
            filtered_list_title(
                "Projects",
                app.filter_query_for(FilterTarget::Projects),
                app.is_filter_editing_target(FilterTarget::Projects),
            ),
            list_focused,
        ))
        .highlight_style(theme.list_highlight())
        .highlight_symbol(HIGHLIGHT_SYMBOL);
    let mut state = ListState::default().with_selected(
        filtered_project_indices
            .iter()
            .position(|idx| *idx == app.project_idx),
    );
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(16), Constraint::Min(6)])
        .split(chunks[1]);

    let rows = if let Some(project) = app.selected_project() {
        let root = display_path(&app.cwd, &project.root);
        let selected_active = app.project_idx == app.active_project_idx;
        vec![
            (String::from("name"), project.name.clone()),
            (
                String::from("root"),
                if root.is_empty() {
                    String::from(".")
                } else {
                    root
                },
            ),
            (String::from("active"), selected_active.to_string()),
            (
                String::from("inventory_sync"),
                project
                    .inventory_sync_cmd
                    .clone()
                    .unwrap_or_else(|| String::from("unset")),
            ),
            (
                String::from("vars_sync"),
                project
                    .vars_sync_cmd
                    .clone()
                    .unwrap_or_else(|| String::from("unset")),
            ),
            (
                String::from("ssh_key_file"),
                project
                    .ssh_private_key_file
                    .clone()
                    .unwrap_or_else(|| String::from("unset")),
            ),
            (
                String::from("ssh_key_inline"),
                summarize_inline_key(project.ssh_private_key_inline.as_deref()),
            ),
            (
                String::from("vault_source"),
                project
                    .vault_source_type
                    .map(|value| value.as_str().to_string())
                    .unwrap_or_else(|| String::from("unset")),
            ),
            (
                String::from("vault_password_file"),
                project
                    .vault_password_file
                    .clone()
                    .unwrap_or_else(|| String::from("unset")),
            ),
            (
                String::from("vault_id_label"),
                project
                    .vault_id_label
                    .clone()
                    .unwrap_or_else(|| String::from("unset")),
            ),
            (
                String::from("playbooks"),
                if selected_active {
                    app.playbooks.len().to_string()
                } else {
                    String::from("(activate to load)")
                },
            ),
            (
                String::from("inventories"),
                if selected_active {
                    app.inventories.len().to_string()
                } else {
                    String::from("(activate to load)")
                },
            ),
        ]
    } else {
        vec![
            (String::from("name"), String::from("none")),
            (String::from("root"), String::from("unset")),
            (String::from("active"), String::from("false")),
            (String::from("inventory_sync"), String::from("unset")),
            (String::from("vars_sync"), String::from("unset")),
            (String::from("ssh_key_file"), String::from("unset")),
            (String::from("ssh_key_inline"), String::from("unset")),
            (String::from("vault_source"), String::from("unset")),
            (String::from("vault_password_file"), String::from("unset")),
            (String::from("vault_id_label"), String::from("unset")),
            (String::from("playbooks"), String::from("0")),
            (String::from("inventories"), String::from("0")),
        ]
    };
    let (detail_cols, detail_spacing) = key_value_table_layout(right[0], 20);
    let details = Table::new(styled_key_value_rows(rows), detail_cols)
        .column_spacing(detail_spacing)
        .block(themed_panel("Project Details", false));
    frame.render_widget(details, right[0]);

    let log_lines = if app.project_sync_logs.is_empty() {
        vec![Line::styled("No project sync logs yet.", theme.text_dim())]
    } else {
        app.project_sync_logs
            .iter()
            .rev()
            .take(right[1].height.saturating_sub(2) as usize)
            .rev()
            .map(|line| Line::raw(line.clone()))
            .collect::<Vec<_>>()
    };
    let logs = Paragraph::new(log_lines)
        .block(
            themed_panel("Project Sync Logs", false).border_style(if app.project_sync_running {
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD)
            } else {
                neutral_border_style()
            }),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(logs, right[1]);
}

pub(super) fn render_project_create_prompt(frame: &mut Frame, app: &App) {
    let theme = th::current();
    let is_git = app.project_create_mode == ProjectCreateMode::Git;
    let area = centered_rect(72, if is_git { 62 } else { 56 }, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = themed_modal(app.project_create_mode.title());
    frame.render_widget(wrapper, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if is_git {
            vec![
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ]
        } else {
            vec![
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(1),
            ]
        })
        .split(inner);

    let intro = match app.project_create_mode {
        ProjectCreateMode::New => {
            "Creates a project root and standard Ansible layout (inventory/playbooks/roles/etc)."
        }
        ProjectCreateMode::ExistingFs => "Registers an existing local project path.",
        ProjectCreateMode::Git => "Clones a git repository and registers it as a project.",
    };
    frame.render_widget(Paragraph::new(intro).style(theme.text_muted()), chunks[0]);

    let field_rows = match app.project_create_mode {
        ProjectCreateMode::New => vec![
            ("Name", app.project_create_buffer_name.clone()),
            ("Root", app.project_create_buffer_root.clone()),
            (
                "Inventory Sync",
                app.project_create_buffer_inventory_sync.clone(),
            ),
            ("Vars Sync", app.project_create_buffer_vars_sync.clone()),
        ],
        ProjectCreateMode::ExistingFs => vec![
            ("Name", app.project_create_buffer_name.clone()),
            ("Existing Root", app.project_create_buffer_root.clone()),
            (
                "Inventory Sync",
                app.project_create_buffer_inventory_sync.clone(),
            ),
            ("Vars Sync", app.project_create_buffer_vars_sync.clone()),
        ],
        ProjectCreateMode::Git => vec![
            ("Name", app.project_create_buffer_name.clone()),
            ("Git URL", app.project_create_buffer_git_url.clone()),
            ("Destination Root", app.project_create_buffer_root.clone()),
            (
                "Inventory Sync",
                app.project_create_buffer_inventory_sync.clone(),
            ),
            ("Vars Sync", app.project_create_buffer_vars_sync.clone()),
        ],
    };
    for (idx, (label, value)) in field_rows.iter().enumerate() {
        let focused = idx == app.project_create_field_idx;
        let display = if focused {
            if value.is_empty() {
                String::from("|")
            } else {
                format!("{value}|")
            }
        } else if value.is_empty() {
            String::from("(empty)")
        } else {
            value.clone()
        };
        frame.render_widget(
            Paragraph::new(display)
                .block(themed_input(*label, focused))
                .style(theme.modal_bg()),
            chunks[idx + 1],
        );
    }

    frame.render_widget(
        Paragraph::new(hint_line_from_bindings(&HINTS_PROJECT_CREATE, 6)).style(hint_bar_style()),
        chunks[chunks.len() - 1],
    );
}

pub(super) fn render_project_ssh_prompt(frame: &mut Frame, app: &App) {
    let theme = th::current();
    let area = centered_rect(78, 72, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = themed_modal("Project Secret Settings");
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
            Constraint::Min(7),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);

    let project_name = app
        .project_ssh_target_name()
        .unwrap_or_else(|| String::from("(unknown)"));
    frame.render_widget(
        Paragraph::new(format!(
            "Project: {project_name} · Vault refs default from project and can be overridden in templates."
        ))
        .style(theme.text_muted()),
        chunks[0],
    );

    let file_focused = app.project_ssh_field_idx == 0;
    let file_display = if file_focused {
        if app.project_ssh_buffer_file.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.project_ssh_buffer_file)
        }
    } else if app.project_ssh_buffer_file.is_empty() {
        String::from("(unset)")
    } else {
        app.project_ssh_buffer_file.clone()
    };
    frame.render_widget(
        Paragraph::new(file_display)
            .block(themed_input("SSH Key File Path", file_focused))
            .style(theme.modal_bg()),
        chunks[1],
    );

    let inline_focused = app.project_ssh_field_idx == 1;
    let inline_display = if inline_focused {
        if app.project_ssh_buffer_inline.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.project_ssh_buffer_inline)
        }
    } else if app.project_ssh_buffer_inline.is_empty() {
        String::from("(unset)")
    } else {
        format!(
            "(set: {} lines, {} chars)",
            app.project_ssh_buffer_inline.lines().count(),
            app.project_ssh_buffer_inline.chars().count()
        )
    };
    frame.render_widget(
        Paragraph::new(inline_display)
            .block(themed_input("Inline SSH Private Key", inline_focused))
            .style(theme.modal_bg())
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    let source_focused = app.project_ssh_field_idx == 2;
    let source_value = app
        .project_vault_source_type
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| String::from("unset"));
    frame.render_widget(
        Paragraph::new(if source_focused {
            format!("{source_value} (h/l/Enter to cycle)")
        } else {
            source_value
        })
        .block(themed_input("Vault Source Type", source_focused))
        .style(theme.modal_bg()),
        chunks[3],
    );

    let vault_file_focused = app.project_ssh_field_idx == 3;
    let vault_file_display = if vault_file_focused {
        if app.project_vault_password_file_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.project_vault_password_file_buffer)
        }
    } else if app.project_vault_password_file_buffer.is_empty() {
        String::from("(unset)")
    } else {
        app.project_vault_password_file_buffer.clone()
    };
    frame.render_widget(
        Paragraph::new(vault_file_display)
            .block(themed_input("Vault Password File", vault_file_focused))
            .style(theme.modal_bg()),
        chunks[4],
    );

    let vault_id_focused = app.project_ssh_field_idx == 4;
    let vault_id_display = if vault_id_focused {
        if app.project_vault_id_label_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.project_vault_id_label_buffer)
        }
    } else if app.project_vault_id_label_buffer.is_empty() {
        String::from("(unset)")
    } else {
        app.project_vault_id_label_buffer.clone()
    };
    frame.render_widget(
        Paragraph::new(vault_id_display)
            .block(themed_input("Vault ID Label", vault_id_focused))
            .style(theme.modal_bg()),
        chunks[5],
    );

    frame.render_widget(
        Paragraph::new(hint_line_from_bindings(&HINTS_PROJECT_SECRETS, 6)).style(hint_bar_style()),
        chunks[6],
    );
}

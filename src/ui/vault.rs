use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Wrap,
};
use ratatui::Frame;

use crate::app::App;
use crate::theme as th;

use super::common::*;
use super::{HINTS_VAULT_CREATE, HINTS_VAULT_EDIT, HINTS_VAULT_PASSWORD_CREATE,
    HINTS_VAULT_PROMPT_CONFIRM, HINTS_VAULT_PROMPT_SIMPLE};

pub(super) fn render_vault_create_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(82, 74, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Create New Encrypted Vault File");
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
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let project_name = app
        .selected_project()
        .map(|project| project.name.clone())
        .unwrap_or_else(|| String::from("(unknown)"));
    frame.render_widget(
        Paragraph::new(format!(
            "Project: {project_name} | Creates a new file only; does not load/decrypt existing vault files."
        ))
        .style(Style::default().fg(th::SUBTEXT1)),
        chunks[0],
    );

    let path_focused = app.vault_create_field_idx == 0;
    let path_display = if path_focused {
        if app.vault_create_buffer_path.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.vault_create_buffer_path)
        }
    } else if app.vault_create_buffer_path.is_empty() {
        String::from("(unset)")
    } else {
        app.vault_create_buffer_path.clone()
    };
    frame.render_widget(
        Paragraph::new(path_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(if path_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Vault File Path"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
        chunks[1],
    );

    let content_focused = app.vault_create_field_idx == 1;
    let content_display = if content_focused {
        if app.vault_create_buffer_content.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.vault_create_buffer_content)
        }
    } else if app.vault_create_buffer_content.is_empty() {
        String::from("(empty)")
    } else {
        format!(
            "(set: {} lines, {} chars)",
            app.vault_create_buffer_content.lines().count(),
            app.vault_create_buffer_content.chars().count()
        )
    };
    frame.render_widget(
        Paragraph::new(content_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(if content_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Vault YAML Content"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    let vault_auth = app
        .selected_project()
        .map(|project| {
            let source = project
                .vault_source_type
                .map(|value| value.as_str().to_string())
                .unwrap_or_else(|| String::from("unset"));
            let password_file = project
                .vault_password_file
                .clone()
                .unwrap_or_else(|| String::from("unset"));
            let vault_id = project
                .vault_id_label
                .clone()
                .unwrap_or_else(|| String::from("unset"));
            format!(
                "Vault auth: source={source} | password_file={password_file} | vault_id={vault_id}"
            )
        })
        .unwrap_or_else(|| String::from("Vault auth: unset"));
    frame.render_widget(
        Paragraph::new(vault_auth)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(neutral_border_style())
                    .title("Auth Source (from Project Secret Settings)"),
            )
            .style(Style::default().fg(th::SUBTEXT1).bg(th::BASE))
            .wrap(Wrap { trim: true }),
        chunks[3],
    );

    frame.render_widget(
        Paragraph::new(hint_line_from_bindings(&HINTS_VAULT_CREATE, 6)).style(hint_bar_style()),
        chunks[4],
    );
}

pub(super) fn render_vault_password_create_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(72, 44, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Create Vault Password File");
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
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let project_name = app
        .selected_project()
        .map(|project| project.name.clone())
        .unwrap_or_else(|| String::from("(unknown)"));
    frame.render_widget(
        Paragraph::new(format!(
            "Project: {project_name} | Creates password file and sets vault source to file."
        ))
        .style(Style::default().fg(th::SUBTEXT1)),
        chunks[0],
    );

    let path_focused = app.vault_password_create_field_idx == 0;
    let path_display = if path_focused {
        if app.vault_password_create_buffer_path.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.vault_password_create_buffer_path)
        }
    } else if app.vault_password_create_buffer_path.is_empty() {
        String::from("(unset)")
    } else {
        app.vault_password_create_buffer_path.clone()
    };
    frame.render_widget(
        Paragraph::new(path_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(if path_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Password File Path"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
        chunks[1],
    );

    let password_focused = app.vault_password_create_field_idx == 1;
    let password_display = if app.vault_password_create_buffer_password.is_empty() {
        if password_focused {
            String::from("|")
        } else {
            String::from("(empty)")
        }
    } else {
        masked_secret(&app.vault_password_create_buffer_password, password_focused)
    };
    frame.render_widget(
        Paragraph::new(password_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(if password_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Vault Password"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
        chunks[2],
    );

    let confirm_focused = app.vault_password_create_field_idx == 2;
    let confirm_display = if app.vault_password_create_buffer_confirm.is_empty() {
        if confirm_focused {
            String::from("|")
        } else {
            String::from("(empty)")
        }
    } else {
        masked_secret(&app.vault_password_create_buffer_confirm, confirm_focused)
    };
    frame.render_widget(
        Paragraph::new(confirm_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(if confirm_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Confirm Password"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
        chunks[3],
    );

    frame.render_widget(
        Paragraph::new(hint_line_from_bindings(&HINTS_VAULT_PASSWORD_CREATE, 6))
            .style(hint_bar_style()),
        chunks[4],
    );
}

pub(super) fn render_vault_edit_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(84, 78, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Edit Encrypted Vault File");
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
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let project_name = app
        .selected_project()
        .map(|project| project.name.clone())
        .unwrap_or_else(|| String::from("(unknown)"));
    frame.render_widget(
        Paragraph::new(format!(
            "Project: {project_name} | Enter on path decrypts+loads; Ctrl+S re-encrypts and saves."
        ))
        .style(Style::default().fg(th::SUBTEXT1)),
        chunks[0],
    );

    let path_focused = app.vault_edit_field_idx == 0;
    let path_display = if path_focused {
        if app.vault_edit_buffer_path.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.vault_edit_buffer_path)
        }
    } else if app.vault_edit_buffer_path.is_empty() {
        String::from("(unset)")
    } else {
        app.vault_edit_buffer_path.clone()
    };
    frame.render_widget(
        Paragraph::new(path_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(if path_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Vault File Path"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
        chunks[1],
    );

    let content_focused = app.vault_edit_field_idx == 1;
    let content_display = if content_focused {
        if app.vault_edit_buffer_content.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.vault_edit_buffer_content)
        }
    } else if app.vault_edit_buffer_content.is_empty() {
        String::from("(empty: press Enter on path to load)")
    } else {
        format!(
            "(loaded: {} lines, {} chars)",
            app.vault_edit_buffer_content.lines().count(),
            app.vault_edit_buffer_content.chars().count()
        )
    };
    frame.render_widget(
        Paragraph::new(content_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(if content_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Decrypted Vault YAML Content"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    let vault_auth = app
        .selected_project()
        .map(|project| {
            let source = project
                .vault_source_type
                .map(|value| value.as_str().to_string())
                .unwrap_or_else(|| String::from("unset"));
            let password_file = project
                .vault_password_file
                .clone()
                .unwrap_or_else(|| String::from("unset"));
            let vault_id = project
                .vault_id_label
                .clone()
                .unwrap_or_else(|| String::from("unset"));
            format!(
                "Vault auth: source={source} | password_file={password_file} | vault_id={vault_id}"
            )
        })
        .unwrap_or_else(|| String::from("Vault auth: unset"));
    frame.render_widget(
        Paragraph::new(if app.vault_edit_loading {
            format!("{vault_auth} | status=loading...")
        } else {
            vault_auth
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if app.vault_edit_loading {
                    Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                } else {
                    neutral_border_style()
                })
                .title("Auth Source (from Project Secret Settings)"),
        )
        .style(Style::default().fg(th::SUBTEXT1).bg(th::BASE))
        .wrap(Wrap { trim: true }),
        chunks[3],
    );

    frame.render_widget(
        Paragraph::new(hint_line_from_bindings(&HINTS_VAULT_EDIT, 6)).style(hint_bar_style()),
        chunks[4],
    );
}

pub(super) fn render_vault_runtime_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(68, 34, frame.area());
    frame.render_widget(Clear, area);
    let confirm_required = app.vault_runtime_prompt_confirm_required();

    let wrapper = Block::default()
        .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Vault Password (Prompt Mode)");
    frame.render_widget(wrapper, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let chunks = if confirm_required {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(2),
            ])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Length(2),
            ])
            .split(inner)
    };

    frame.render_widget(
        Paragraph::new("Enter vault password. It will be reused for this project session.")
            .style(Style::default().fg(th::SUBTEXT1)),
        chunks[0],
    );

    let password_focused = app.vault_runtime_prompt_field_idx == 0;
    let password_display = if app.vault_runtime_prompt_password.is_empty() {
        if password_focused {
            String::from("|")
        } else {
            String::from("(empty)")
        }
    } else {
        masked_secret(&app.vault_runtime_prompt_password, password_focused)
    };
    frame.render_widget(
        Paragraph::new(password_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                    .border_style(if password_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Vault Password"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
        chunks[1],
    );

    if confirm_required {
        let confirm_focused = app.vault_runtime_prompt_field_idx == 1;
        let confirm_display = if app.vault_runtime_prompt_confirm.is_empty() {
            if confirm_focused {
                String::from("|")
            } else {
                String::from("(empty)")
            }
        } else {
            masked_secret(&app.vault_runtime_prompt_confirm, confirm_focused)
        };
        frame.render_widget(
            Paragraph::new(confirm_display)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                        .border_style(if confirm_focused {
                            Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                        } else {
                            neutral_border_style()
                        })
                        .title("Confirm Password"),
                )
                .style(Style::default().fg(th::TEXT).bg(th::BASE)),
            chunks[2],
        );
    }

    frame.render_widget(
        Paragraph::new(if confirm_required {
            hint_line_from_bindings(&HINTS_VAULT_PROMPT_CONFIRM, 6)
        } else {
            hint_line_from_bindings(&HINTS_VAULT_PROMPT_SIMPLE, 6)
        })
        .style(hint_bar_style()),
        if confirm_required {
            chunks[3]
        } else {
            chunks[2]
        },
    );
}


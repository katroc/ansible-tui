use std::collections::HashSet;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Tabs,
};
use ratatui::Frame;

use crate::app::{
    App, FocusContext, InventorySubTab,
    View,
};
use crate::run::playbook_bin_available;
use crate::theme as th;

mod common;
mod dashboard;
mod inventory;
mod playbooks;
mod projects;
mod settings_view;
mod templates;
mod vault;

use common::*;
use dashboard::*;
use inventory::*;
use playbooks::*;
use projects::*;
use settings_view::*;
use templates::*;
use vault::*;

#[derive(Clone, Copy)]
struct HintBinding {
    key: &'static str,
    desc: &'static str,
}

#[derive(Clone, Copy)]
struct HelpModel {
    title: &'static str,
    hints: &'static [HintBinding],
}

const HINTS_RUNTIME_PROMPT: [HintBinding; 4] = [
    HintBinding {
        key: "j/k, Up/Down",
        desc: "select runtime",
    },
    HintBinding {
        key: "Enter",
        desc: "use selected runtime",
    },
    HintBinding {
        key: "b",
        desc: "bootstrap managed runtime",
    },
    HintBinding {
        key: "Esc",
        desc: "close picker",
    },
];
const HINTS_VAULT_PROMPT_CONFIRM: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "enter password text",
    },
    HintBinding {
        key: "Tab/Shift+Tab",
        desc: "switch field",
    },
    HintBinding {
        key: "Enter",
        desc: "next/confirm",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "continue action",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_VAULT_PROMPT_SIMPLE: [HintBinding; 4] = [
    HintBinding {
        key: "Type",
        desc: "enter password text",
    },
    HintBinding {
        key: "Enter",
        desc: "confirm",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "continue action",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_INVENTORY_CREATE: [HintBinding; 4] = [
    HintBinding {
        key: "Type",
        desc: "inventory filename",
    },
    HintBinding {
        key: "Enter",
        desc: "create file",
    },
    HintBinding {
        key: "Backspace",
        desc: "edit text",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_PROJECT_CREATE: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "edit current field",
    },
    HintBinding {
        key: "Up/Down",
        desc: "change field",
    },
    HintBinding {
        key: "Enter",
        desc: "next/save",
    },
    HintBinding {
        key: "Backspace",
        desc: "edit text",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_PROJECT_SECRETS: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "edit current field",
    },
    HintBinding {
        key: "Up/Down",
        desc: "change field",
    },
    HintBinding {
        key: "h/l",
        desc: "cycle vault source",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "save settings",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_VAULT_CREATE: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "edit path/content",
    },
    HintBinding {
        key: "Up/Down",
        desc: "switch field",
    },
    HintBinding {
        key: "Enter",
        desc: "next/newline",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "encrypt and save",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_VAULT_EDIT: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "edit path/content",
    },
    HintBinding {
        key: "Up/Down",
        desc: "switch field",
    },
    HintBinding {
        key: "Enter",
        desc: "reload/newline",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "re-encrypt and save",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_VAULT_PASSWORD_CREATE: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "edit fields",
    },
    HintBinding {
        key: "Up/Down",
        desc: "switch field",
    },
    HintBinding {
        key: "Enter",
        desc: "next field",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "create file",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_INVENTORY_EDIT_MODE: [HintBinding; 5] = [
    HintBinding {
        key: "j/k, Up/Down",
        desc: "select mode",
    },
    HintBinding {
        key: "Enter",
        desc: "confirm mode",
    },
    HintBinding {
        key: "1/2, e/t",
        desc: "quick select",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
    HintBinding {
        key: "Tab/h/l",
        desc: "switch view",
    },
];
const HINTS_INVENTORY_EDITOR: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "edit text",
    },
    HintBinding {
        key: "Enter",
        desc: "newline",
    },
    HintBinding {
        key: "Backspace",
        desc: "delete char",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "save",
    },
    HintBinding {
        key: "Esc",
        desc: "close editor",
    },
];
const HINTS_SETTINGS_EDITOR: [HintBinding; 5] = [
    HintBinding {
        key: "j/k",
        desc: "move field",
    },
    HintBinding {
        key: "h/l, <-/->",
        desc: "adjust value",
    },
    HintBinding {
        key: "Enter/e",
        desc: "edit text field",
    },
    HintBinding {
        key: "Space",
        desc: "toggle boolean",
    },
    HintBinding {
        key: "Esc/t",
        desc: "close",
    },
];
const HINTS_SETTINGS_EDITOR_TEXT: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "edit text",
    },
    HintBinding {
        key: "Enter",
        desc: "save/newline",
    },
    HintBinding {
        key: "Backspace",
        desc: "delete char",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "save",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_TEMPLATE_EDITOR: [HintBinding; 5] = [
    HintBinding {
        key: "j/k",
        desc: "move field",
    },
    HintBinding {
        key: "h/l, <-/->",
        desc: "adjust value",
    },
    HintBinding {
        key: "Enter/e",
        desc: "edit text field",
    },
    HintBinding {
        key: "Space",
        desc: "toggle boolean",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "save template",
    },
];
const HINTS_TEMPLATE_EDITOR_TEXT: [HintBinding; 5] = [
    HintBinding {
        key: "Type",
        desc: "edit text",
    },
    HintBinding {
        key: "Enter",
        desc: "save/newline",
    },
    HintBinding {
        key: "Backspace",
        desc: "delete char",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "save",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel",
    },
];
const HINTS_DASHBOARD: [HintBinding; 5] = [
    HintBinding {
        key: "Tab/h/l",
        desc: "switch view",
    },
    HintBinding {
        key: "r",
        desc: "run selected playbook",
    },
    HintBinding {
        key: "u",
        desc: "open runtime picker",
    },
    HintBinding {
        key: "Shift+R",
        desc: "refresh project discovery",
    },
    HintBinding {
        key: "q",
        desc: "quit",
    },
];
const HINTS_PROJECTS: [HintBinding; 8] = [
    HintBinding {
        key: "j/k, Up/Down",
        desc: "select project",
    },
    HintBinding {
        key: "Enter/a",
        desc: "activate selected",
    },
    HintBinding {
        key: "n/f/g",
        desc: "new/import/clone project",
    },
    HintBinding {
        key: "e",
        desc: "project secret settings",
    },
    HintBinding {
        key: "Shift+V/E/P",
        desc: "create/edit vault, create password file",
    },
    HintBinding {
        key: "i/v",
        desc: "sync inventory/vars",
    },
    HintBinding {
        key: "Shift+D",
        desc: "delete selected project",
    },
    HintBinding {
        key: "/",
        desc: "filter list (Esc clears)",
    },
];
const HINTS_INVENTORY_FILES: [HintBinding; 7] = [
    HintBinding {
        key: "j/k, Up/Down",
        desc: "select inventory",
    },
    HintBinding {
        key: "n",
        desc: "new inventory",
    },
    HintBinding {
        key: "e",
        desc: "edit selected inventory",
    },
    HintBinding {
        key: "Shift+D",
        desc: "delete selected inventory",
    },
    HintBinding {
        key: "2/3",
        desc: "switch to Hosts/Groups",
    },
    HintBinding {
        key: "Tab/h/l",
        desc: "switch view",
    },
    HintBinding {
        key: "/",
        desc: "filter list (Esc clears)",
    },
];
const HINTS_INVENTORY_HOSTS: [HintBinding; 7] = [
    HintBinding {
        key: "h/l",
        desc: "focus list/detail",
    },
    HintBinding {
        key: "j/k",
        desc: "navigate focused panel",
    },
    HintBinding {
        key: "e, Enter",
        desc: "edit selected field",
    },
    HintBinding {
        key: "n/a",
        desc: "add host / variable",
    },
    HintBinding {
        key: "D",
        desc: "delete host or variable",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "save inventory",
    },
    HintBinding {
        key: "Esc",
        desc: "back to inventory files",
    },
];
const HINTS_INVENTORY_GROUPS: [HintBinding; 7] = [
    HintBinding {
        key: "h/l",
        desc: "focus tree/groups/hosts",
    },
    HintBinding {
        key: "j/k",
        desc: "navigate focused list",
    },
    HintBinding {
        key: "Space",
        desc: "toggle attach/detach",
    },
    HintBinding {
        key: "n",
        desc: "add group/host",
    },
    HintBinding {
        key: "d, D",
        desc: "detach / delete",
    },
    HintBinding {
        key: "Ctrl+S",
        desc: "save inventory",
    },
    HintBinding {
        key: "Esc",
        desc: "back to inventory files",
    },
];
const HINTS_PLAYBOOKS: [HintBinding; 8] = [
    HintBinding {
        key: "<-/->, h/l, Enter",
        desc: "focus playbooks/runs",
    },
    HintBinding {
        key: "j/k",
        desc: "move focused list",
    },
    HintBinding {
        key: "i/I",
        desc: "cycle inventory target",
    },
    HintBinding {
        key: "r",
        desc: "run selected playbook",
    },
    HintBinding {
        key: "t",
        desc: "playbook settings",
    },
    HintBinding {
        key: "v",
        desc: "log-select mode",
    },
    HintBinding {
        key: "PgUp/PgDn, End",
        desc: "scroll/follow logs",
    },
    HintBinding {
        key: "/",
        desc: "filter focused list",
    },
];
const HINTS_PLAYBOOKS_LOG_SELECT: [HintBinding; 8] = [
    HintBinding {
        key: "j/k, Up/Down",
        desc: "move log cursor",
    },
    HintBinding {
        key: "Space",
        desc: "set/clear selection mark",
    },
    HintBinding {
        key: "y",
        desc: "copy selected log lines",
    },
    HintBinding {
        key: "v",
        desc: "exit log-select mode",
    },
    HintBinding {
        key: "PgUp/PgDn, End",
        desc: "scroll/follow logs",
    },
    HintBinding {
        key: "<-/->",
        desc: "focus playbooks/runs",
    },
    HintBinding {
        key: "r",
        desc: "run selected playbook",
    },
    HintBinding {
        key: "/",
        desc: "filter focused list",
    },
];
const HINTS_TEMPLATES: [HintBinding; 8] = [
    HintBinding {
        key: "<-/->, h/l, Enter",
        desc: "focus templates/runs",
    },
    HintBinding {
        key: "j/k",
        desc: "move focused list",
    },
    HintBinding {
        key: "n",
        desc: "new template",
    },
    HintBinding {
        key: "t/e",
        desc: "edit template",
    },
    HintBinding {
        key: "r",
        desc: "run selected template",
    },
    HintBinding {
        key: "Shift+D",
        desc: "delete selected template",
    },
    HintBinding {
        key: "v",
        desc: "log-select mode",
    },
    HintBinding {
        key: "/",
        desc: "filter focused list",
    },
];
const HINTS_TEMPLATES_LOG_SELECT: [HintBinding; 7] = [
    HintBinding {
        key: "j/k, Up/Down",
        desc: "move log cursor",
    },
    HintBinding {
        key: "Space",
        desc: "set/clear selection mark",
    },
    HintBinding {
        key: "y",
        desc: "copy selected log lines",
    },
    HintBinding {
        key: "v",
        desc: "exit log-select mode",
    },
    HintBinding {
        key: "<-/->",
        desc: "focus templates/runs",
    },
    HintBinding {
        key: "r",
        desc: "run selected template",
    },
    HintBinding {
        key: "/",
        desc: "filter focused list",
    },
];
const HINTS_SETTINGS: [HintBinding; 6] = [
    HintBinding {
        key: "j/k",
        desc: "select field",
    },
    HintBinding {
        key: "h/l, <-/->",
        desc: "adjust selected value",
    },
    HintBinding {
        key: "Space",
        desc: "toggle boolean",
    },
    HintBinding {
        key: "Enter/e",
        desc: "edit text field",
    },
    HintBinding {
        key: "u",
        desc: "open runtime picker",
    },
    HintBinding {
        key: "Tab",
        desc: "switch view",
    },
];
const HINTS_SETTINGS_TEXT: [HintBinding; 6] = [
    HintBinding {
        key: "Type",
        desc: "edit text value",
    },
    HintBinding {
        key: "Backspace",
        desc: "delete char",
    },
    HintBinding {
        key: "Enter",
        desc: "save value",
    },
    HintBinding {
        key: "Esc",
        desc: "cancel edit",
    },
    HintBinding {
        key: "j/k",
        desc: "move field",
    },
    HintBinding {
        key: "Tab",
        desc: "switch view",
    },
];
const HINTS_LIST_FILTER_EDIT: [HintBinding; 4] = [
    HintBinding {
        key: "Type",
        desc: "edit filter query",
    },
    HintBinding {
        key: "Backspace",
        desc: "delete char",
    },
    HintBinding {
        key: "Enter",
        desc: "apply filter",
    },
    HintBinding {
        key: "Esc",
        desc: "clear filter",
    },
];

pub fn render(frame: &mut Frame, app: &App) {
    frame.render_widget(
        Block::default().style(Style::default().bg(th::BASE)),
        frame.area(),
    );

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_tabs(frame, app, layout[0]);
    render_body(frame, app, layout[1]);
    render_help(frame, app, layout[2]);
    render_status(frame, app, layout[3]);
    if app.settings_editor_open {
        render_playbook_settings_editor(frame, app);
    }
    if app.template_editor_open {
        render_template_editor(frame, app);
    }
    if app.inventory_create_open {
        render_inventory_create_prompt(frame, app);
    }
    if app.project_create_open {
        render_project_create_prompt(frame, app);
    }
    if app.project_ssh_open {
        render_project_ssh_prompt(frame, app);
    }
    if app.vault_create_open {
        render_vault_create_prompt(frame, app);
    }
    if app.vault_edit_open {
        render_vault_edit_prompt(frame, app);
    }
    if app.vault_runtime_prompt_open {
        render_vault_runtime_prompt(frame, app);
    }
    if app.vault_password_create_open {
        render_vault_password_create_prompt(frame, app);
    }
    if app.inventory_edit_mode_open {
        render_inventory_edit_mode_prompt(frame, app);
    }
    if app.inventory_editor_open {
        render_inventory_editor(frame, app);
    }
    if app.runtime_prompt_open {
        render_runtime_prompt(frame, app);
    }
    if app.help_overlay_open {
        render_help_overlay(frame, app);
    }
}

fn render_body(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.current_view() == View::Dashboard || app.current_view() == View::Projects {
        render_main(frame, app, area);
        return;
    }

    let constraints = if app.current_view() == View::Inventory
        && !matches!(app.inventory_sub_tab, InventorySubTab::Files)
    {
        [Constraint::Percentage(65), Constraint::Percentage(35)]
    } else {
        [Constraint::Percentage(40), Constraint::Percentage(60)]
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    render_main(frame, app, columns[0]);
    render_logs(frame, app, columns[1]);
}

fn render_tabs(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let titles = View::all()
        .iter()
        .map(|v| Line::from(Span::raw(v.title())))
        .collect::<Vec<_>>();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .style(Style::default().bg(th::MANTLE).fg(th::SUBTEXT1))
                .title("Ansible TUI"),
        )
        .select(app.view_idx)
        .highlight_style(Style::default().fg(th::MAUVE).add_modifier(Modifier::BOLD))
        .style(Style::default().fg(th::SUBTEXT0));

    frame.render_widget(tabs, area);
}

fn render_main(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    match app.current_view() {
        View::Dashboard => render_dashboard(frame, app, area),
        View::Projects => render_projects(frame, app, area),
        View::Inventory => render_inventory(frame, app, area),
        View::Playbooks => render_playbooks(frame, app, area),
        View::Templates => render_templates(frame, app, area),
        View::Settings => render_settings(frame, app, area),
    }
}

fn render_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let runtime_required = !playbook_bin_available(&app.run_options.ansible_bin);
    let style = status_line_style(app, runtime_required);
    let status = Paragraph::new(app.status_line.clone()).style(style);
    frame.render_widget(status, area);
}

fn render_help(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let helper = Paragraph::new(compact_help_line(app)).style(hint_bar_style());
    frame.render_widget(helper, area);
}

fn render_help_overlay(frame: &mut Frame, app: &App) {
    let model = active_help_model(app);
    let hints = active_hint_bindings(app);
    let focus_label = focus_context_label(app.content_focus_context());

    let area = centered_rect(86, 82, frame.area());
    frame.render_widget(Clear, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(area);

    let header = Paragraph::new(format!(
        "Context: {}  |  Focus: {}",
        model.title, focus_label
    ))
    .style(
        Style::default()
            .fg(th::TEXT)
            .bg(th::SURFACE1)
            .add_modifier(Modifier::BOLD),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(th::FOCUS_BORDER))
            .style(Style::default().bg(th::MANTLE))
            .title("Keyboard Help"),
    );
    frame.render_widget(header, layout[0]);

    let mut lines = hints
        .iter()
        .map(|hint| {
            Line::from(vec![
                Span::styled(
                    format!("{:<18}", hint.key),
                    Style::default()
                        .fg(th::HINT_KEY)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(hint.desc, Style::default().fg(th::TEXT)),
            ])
        })
        .collect::<Vec<_>>();

    let max_rows = layout[1].height.saturating_sub(2) as usize;
    if lines.len() > max_rows {
        lines.truncate(max_rows);
    }

    let body = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
            .border_style(neutral_border_style())
            .style(Style::default().bg(th::BASE)),
    );
    frame.render_widget(body, layout[1]);

    let footer = Paragraph::new("Esc or ? close help").style(hint_bar_style());
    frame.render_widget(footer, layout[2]);
}

fn compact_help_line(app: &App) -> Line<'static> {
    let hints = active_hint_bindings(app);
    let mut spans = vec![
        Span::styled(
            format!("{} ", focus_context_label(app.content_focus_context())),
            Style::default().fg(th::SUBTEXT1),
        ),
        Span::styled("| ", Style::default().fg(th::SUBTEXT1)),
    ];
    spans.extend(hint_spans_from_bindings(&hints, 5));
    Line::from(spans)
}

fn active_help_model(app: &App) -> HelpModel {
    if app.runtime_prompt_open {
        return HelpModel {
            title: "Runtime Picker",
            hints: &HINTS_RUNTIME_PROMPT,
        };
    }
    if app.vault_runtime_prompt_open {
        return if app.vault_runtime_prompt_confirm_required() {
            HelpModel {
                title: "Vault Password Prompt",
                hints: &HINTS_VAULT_PROMPT_CONFIRM,
            }
        } else {
            HelpModel {
                title: "Vault Password Prompt",
                hints: &HINTS_VAULT_PROMPT_SIMPLE,
            }
        };
    }
    if app.inventory_create_open {
        return HelpModel {
            title: "Inventory Create",
            hints: &HINTS_INVENTORY_CREATE,
        };
    }
    if app.project_create_open {
        return HelpModel {
            title: "Project Create",
            hints: &HINTS_PROJECT_CREATE,
        };
    }
    if app.project_ssh_open {
        return HelpModel {
            title: "Project Secret Settings",
            hints: &HINTS_PROJECT_SECRETS,
        };
    }
    if app.vault_create_open {
        return HelpModel {
            title: "Vault Create",
            hints: &HINTS_VAULT_CREATE,
        };
    }
    if app.vault_edit_open {
        return HelpModel {
            title: "Vault Edit",
            hints: &HINTS_VAULT_EDIT,
        };
    }
    if app.vault_password_create_open {
        return HelpModel {
            title: "Vault Password Helper",
            hints: &HINTS_VAULT_PASSWORD_CREATE,
        };
    }
    if app.inventory_edit_mode_open {
        return HelpModel {
            title: "Inventory Edit Mode Picker",
            hints: &HINTS_INVENTORY_EDIT_MODE,
        };
    }
    if app.inventory_editor_open {
        return HelpModel {
            title: "Inventory Editor",
            hints: &HINTS_INVENTORY_EDITOR,
        };
    }
    if app.filter_edit_mode {
        return HelpModel {
            title: "List Filter",
            hints: &HINTS_LIST_FILTER_EDIT,
        };
    }
    if app.current_view() == View::Settings && app.global_settings_text_mode {
        return HelpModel {
            title: "Global Settings Text Edit",
            hints: &HINTS_SETTINGS_TEXT,
        };
    }
    if app.settings_editor_open {
        return if app.settings_editor_text_mode {
            HelpModel {
                title: "Playbook Settings Text Edit",
                hints: &HINTS_SETTINGS_EDITOR_TEXT,
            }
        } else {
            HelpModel {
                title: "Playbook Settings Editor",
                hints: &HINTS_SETTINGS_EDITOR,
            }
        };
    }
    if app.template_editor_open {
        return if app.template_editor_text_mode {
            HelpModel {
                title: "Template Editor Text Edit",
                hints: &HINTS_TEMPLATE_EDITOR_TEXT,
            }
        } else {
            HelpModel {
                title: "Template Editor",
                hints: &HINTS_TEMPLATE_EDITOR,
            }
        };
    }

    match app.content_focus_context() {
        FocusContext::Dashboard => HelpModel {
            title: "Dashboard",
            hints: &HINTS_DASHBOARD,
        },
        FocusContext::Projects => HelpModel {
            title: "Projects",
            hints: &HINTS_PROJECTS,
        },
        FocusContext::InventoryFiles => HelpModel {
            title: "Inventory Files",
            hints: &HINTS_INVENTORY_FILES,
        },
        FocusContext::InventoryHostsList | FocusContext::InventoryHostDetails => HelpModel {
            title: "Inventory Hosts",
            hints: &HINTS_INVENTORY_HOSTS,
        },
        FocusContext::InventoryGroupsTree
        | FocusContext::InventoryGroupsGroups
        | FocusContext::InventoryGroupsHosts => HelpModel {
            title: "Inventory Groups",
            hints: &HINTS_INVENTORY_GROUPS,
        },
        FocusContext::PlaybooksList | FocusContext::PlaybooksRuns => HelpModel {
            title: "Playbooks",
            hints: &HINTS_PLAYBOOKS,
        },
        FocusContext::PlaybooksLogSelect => HelpModel {
            title: "Playbooks (Log Select)",
            hints: &HINTS_PLAYBOOKS_LOG_SELECT,
        },
        FocusContext::TemplatesList | FocusContext::TemplatesRuns => HelpModel {
            title: "Templates",
            hints: &HINTS_TEMPLATES,
        },
        FocusContext::TemplatesLogSelect => HelpModel {
            title: "Templates (Log Select)",
            hints: &HINTS_TEMPLATES_LOG_SELECT,
        },
        FocusContext::Settings => HelpModel {
            title: "Global Settings",
            hints: &HINTS_SETTINGS,
        },
        FocusContext::RuntimePrompt => HelpModel {
            title: "Runtime Picker",
            hints: &HINTS_RUNTIME_PROMPT,
        },
        FocusContext::Modal => match app.current_view() {
            View::Settings => HelpModel {
                title: "Global Settings",
                hints: &HINTS_SETTINGS,
            },
            View::Templates => HelpModel {
                title: "Templates",
                hints: &HINTS_TEMPLATES,
            },
            View::Playbooks => HelpModel {
                title: "Playbooks",
                hints: &HINTS_PLAYBOOKS,
            },
            View::Inventory => HelpModel {
                title: "Inventory",
                hints: &HINTS_INVENTORY_FILES,
            },
            View::Projects => HelpModel {
                title: "Projects",
                hints: &HINTS_PROJECTS,
            },
            View::Dashboard => HelpModel {
                title: "Dashboard",
                hints: &HINTS_DASHBOARD,
            },
        },
    }
}

fn active_hint_bindings(app: &App) -> Vec<HintBinding> {
    let model = active_help_model(app);
    let mut hints = model.hints.to_vec();
    if help_toggle_available(app) {
        hints.push(HintBinding {
            key: "?",
            desc: "toggle keyboard help",
        });
    }

    let mut deduped = Vec::with_capacity(hints.len());
    let mut seen = HashSet::new();
    for hint in hints {
        let marker = format!("{}::{}", hint.key, hint.desc);
        if seen.insert(marker) {
            deduped.push(hint);
        }
    }
    deduped
}

fn help_toggle_available(app: &App) -> bool {
    !(app.vault_runtime_prompt_open
        || (app.settings_editor_open && app.settings_editor_text_mode)
        || (app.template_editor_open && app.template_editor_text_mode)
        || app.filter_edit_mode
        || app.project_ssh_open
        || app.vault_create_open
        || app.vault_edit_open
        || app.vault_password_create_open
        || app.project_create_open
        || app.inventory_create_open
        || app.inventory_editor_open
        || app.hosts_subtab_editing
        || app.hosts_subtab_add_host_open
        || app.hosts_subtab_add_var_open
        || (app.current_view() == View::Settings && app.global_settings_text_mode))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    fn make_test_app(name: &str) -> App {
        let cwd = std::env::temp_dir().join(format!("ansible_tui_ui_tests_{name}"));
        let _ = std::fs::create_dir_all(&cwd);
        App::new(cwd)
    }

    #[test]
    fn hint_bindings_include_help_toggle_when_available() {
        let app = make_test_app("help_toggle_on");
        let hints = active_hint_bindings(&app);
        assert!(hints
            .iter()
            .any(|hint| hint.key == "?" && hint.desc == "toggle keyboard help"));
    }

    #[test]
    fn hint_bindings_hide_help_toggle_in_text_mode() {
        let mut app = make_test_app("help_toggle_off");
        app.inventory_create_open = true;
        let hints = active_hint_bindings(&app);
        assert!(!hints.iter().any(|hint| hint.key == "?"));
    }

    #[test]
    fn focus_context_label_for_playbooks_runs() {
        let mut app = make_test_app("focus_label");
        app.runtime_prompt_open = false;
        app.view_idx = 3;
        app.playbooks_focus_runs = true;
        assert_eq!(
            focus_context_label(app.content_focus_context()),
            "playbook runs"
        );
    }
}

use std::collections::{BTreeMap, HashSet};
use std::fs;

use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    BarChart, Block, Borders, Cell, Clear, Gauge, List, ListItem, ListState, Paragraph, Row,
    Sparkline, Table, TableState, Tabs, Wrap,
};
use ratatui::Frame;

use crate::app::{
    display_path, App, FilterTarget, FocusContext, InventorySubTab, ProjectCreateMode, RunStatus,
    View,
};
use crate::playbook_settings::PlaybookSettings;
use crate::run::playbook_bin_available;
use crate::theme as th;

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

fn render_dashboard(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let running = app
        .runs
        .iter()
        .filter(|r| matches!(r.status, RunStatus::Running))
        .count();
    let succeeded = app
        .runs
        .iter()
        .filter(|r| matches!(r.status, RunStatus::Succeeded))
        .count();
    let failed = app
        .runs
        .iter()
        .filter(|r| matches!(r.status, RunStatus::Failed))
        .count();
    let completed = succeeded + failed;
    let total_runs = app.runs.len();

    let runtime_ready = app
        .runtime_candidates
        .iter()
        .find(|candidate| candidate.ansible_bin == app.run_options.ansible_bin)
        .map(|candidate| candidate.available)
        .unwrap_or(false);
    let runtime_percent: u16 = if runtime_ready { 100 } else { 0 };
    let success_percent: u16 = if completed == 0 {
        0
    } else {
        ((succeeded as f64 / completed as f64) * 100.0).round() as u16
    };
    let available_playbooks = app
        .playbooks
        .iter()
        .map(|path| display_path(app.active_project_root(), path))
        .collect::<HashSet<_>>();
    let playbooks_with_runs = app
        .runs
        .iter()
        .filter_map(|run| {
            if available_playbooks.contains(&run.playbook) {
                Some(run.playbook.clone())
            } else {
                None
            }
        })
        .collect::<HashSet<_>>()
        .len();
    let coverage_percent: u16 = if app.playbooks.is_empty() {
        0
    } else {
        (((playbooks_with_runs as f64 / app.playbooks.len() as f64) * 100.0).round() as u16)
            .min(100)
    };

    let mut runs_by_playbook = BTreeMap::<String, u64>::new();
    let mut runs_by_inventory = BTreeMap::<String, u64>::new();
    for run in &app.runs {
        *runs_by_playbook.entry(run.playbook.clone()).or_insert(0) += 1;
        *runs_by_inventory.entry(run.inventory.clone()).or_insert(0) += 1;
    }
    let (top_playbooks, playbook_max) = top_ranked_items(&runs_by_playbook, 6);
    let (top_inventories, inventory_max) = top_ranked_items(&runs_by_inventory, 6);

    let mut outcome_points = app
        .runs
        .iter()
        .take(40)
        .map(|run| match run.status {
            RunStatus::Running => 60_u64,
            RunStatus::Succeeded => 100_u64,
            RunStatus::Failed => 20_u64,
        })
        .collect::<Vec<_>>();
    outcome_points.reverse();
    if outcome_points.is_empty() {
        outcome_points.push(0);
    }

    let today = chrono::Local::now().date_naive();
    let mut daily_points = vec![0_u64; 14];
    for run in &app.runs {
        let days_ago = (today - run.started_at.date_naive()).num_days();
        if (0..14).contains(&days_ago) {
            let idx = 13_usize.saturating_sub(days_ago as usize);
            daily_points[idx] += 1;
        }
    }
    if daily_points.iter().all(|v| *v == 0) {
        daily_points[13] = total_runs as u64;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(10),
        ])
        .split(area);

    let summary = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(root[0]);
    frame.render_widget(
        dashboard_stat_card("Inventories", app.inventories.len().to_string(), th::YELLOW),
        summary[0],
    );
    frame.render_widget(
        dashboard_stat_card("Playbooks", app.playbooks.len().to_string(), th::MAUVE),
        summary[1],
    );
    frame.render_widget(
        dashboard_stat_card("Total Runs", total_runs.to_string(), th::GREEN),
        summary[2],
    );
    frame.render_widget(
        dashboard_stat_card(
            "Failing Runs",
            failed.to_string(),
            if failed > 0 { th::RED } else { th::GREEN },
        ),
        summary[3],
    );

    let gauges = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(root[1]);
    let runtime_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Runtime Health"),
        )
        .label(if runtime_ready {
            Span::styled(
                "ready",
                Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                "missing",
                Style::default().fg(th::RED).add_modifier(Modifier::BOLD),
            )
        })
        .gauge_style(Style::default().fg(if runtime_ready { th::GREEN } else { th::RED }))
        .use_unicode(true)
        .percent(runtime_percent);
    frame.render_widget(runtime_gauge, gauges[0]);

    let success_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Success Rate"),
        )
        .label(Span::styled(
            format!("{success_percent}%"),
            Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD),
        ))
        .gauge_style(Style::default().fg(th::GREEN))
        .use_unicode(true)
        .percent(success_percent);
    frame.render_widget(success_gauge, gauges[1]);

    let coverage_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Playbook Coverage"),
        )
        .label(Span::styled(
            format!("{coverage_percent}%"),
            Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD),
        ))
        .gauge_style(Style::default().fg(th::YELLOW))
        .use_unicode(true)
        .percent(coverage_percent);
    frame.render_widget(coverage_gauge, gauges[2]);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(root[2]);

    let status_max = (running.max(succeeded).max(failed) as u64).max(1);
    let status_chart = BarChart::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Job Status Breakdown"),
        )
        .bar_width(8)
        .bar_gap(2)
        .value_style(Style::default().fg(th::TEXT).add_modifier(Modifier::BOLD))
        .label_style(Style::default().fg(th::SUBTEXT1))
        .bar_style(Style::default().fg(th::MAUVE))
        .data(&[
            ("run", running as u64),
            ("ok", succeeded as u64),
            ("fail", failed as u64),
        ])
        .max(status_max);
    frame.render_widget(status_chart, bottom[0]);

    let trends = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(bottom[1]);
    let outcomes = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Run Outcomes (40)"),
        )
        .style(Style::default().fg(th::GREEN))
        .max(100)
        .data(outcome_points);
    frame.render_widget(outcomes, trends[0]);

    let run_volume_max = daily_points.iter().copied().max().unwrap_or(1).max(1);
    let volume = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Run Volume (14d)"),
        )
        .style(Style::default().fg(th::YELLOW))
        .max(run_volume_max)
        .data(daily_points);
    frame.render_widget(volume, trends[1]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(bottom[2]);
    render_ranked_bar_panel(
        frame,
        right[0],
        "Top Playbooks",
        &top_playbooks,
        playbook_max.max(1),
        th::GREEN,
    );
    render_ranked_bar_panel(
        frame,
        right[1],
        "Top Inventories",
        &top_inventories,
        inventory_max.max(1),
        th::MAUVE,
    );
    render_project_summary_panel(frame, right[2], app, running, succeeded, failed, total_runs);
}

fn render_projects(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title(filtered_list_title(
                    "Projects",
                    app.filter_query_for(FilterTarget::Projects),
                    app.is_filter_editing_target(FilterTarget::Projects),
                )),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .style(Style::default().bg(th::BASE))
                .title("Project Details"),
        );
    frame.render_widget(details, right[0]);

    let log_lines = if app.project_sync_logs.is_empty() {
        vec![Line::styled(
            "No project sync logs yet.",
            Style::default().fg(th::SUBTEXT0),
        )]
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
            Block::default()
                .borders(Borders::ALL)
                .border_style(if app.project_sync_running {
                    Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                } else {
                    neutral_border_style()
                })
                .title("Project Sync Logs"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(logs, right[1]);
}

fn render_inventory(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(area);

    let is_yaml = app.selected_inventory_is_yaml();
    let tab_titles: Vec<Line> = vec![
        Line::from("1:Files"),
        Line::from(Span::styled(
            "2:Hosts",
            if is_yaml {
                Style::default()
            } else {
                Style::default().fg(th::SURFACE1)
            },
        )),
        Line::from(Span::styled(
            "3:Groups",
            if is_yaml {
                Style::default()
            } else {
                Style::default().fg(th::SURFACE1)
            },
        )),
    ];
    let selected_tab = match app.inventory_sub_tab {
        InventorySubTab::Files => 0,
        InventorySubTab::Hosts => 1,
        InventorySubTab::Groups => 2,
    };
    let sub_tabs = Tabs::new(tab_titles)
        .select(selected_tab)
        .highlight_style(Style::default().fg(th::MAUVE).add_modifier(Modifier::BOLD))
        .style(Style::default().fg(th::SUBTEXT0))
        .divider("|");
    frame.render_widget(sub_tabs, layout[0]);

    match app.inventory_sub_tab {
        InventorySubTab::Files => render_inventory_files(frame, app, layout[1]),
        InventorySubTab::Hosts => render_inventory_hosts(frame, app, layout[1]),
        InventorySubTab::Groups => render_inventory_groups(frame, app, layout[1]),
    }
}

fn render_inventory_files(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(area);

    let filtered_inventory_indices = app.filtered_inventory_indices();
    let items = if app.inventories.is_empty() {
        vec![ListItem::new("No inventories found under ./inventory")]
    } else if filtered_inventory_indices.is_empty() {
        vec![ListItem::new("No inventories match current filter")]
    } else {
        filtered_inventory_indices
            .iter()
            .map(|idx| {
                ListItem::new(display_path(
                    app.active_project_root(),
                    &app.inventories[*idx],
                ))
            })
            .collect::<Vec<_>>()
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title(filtered_list_title(
                    "Inventories",
                    app.filter_query_for(FilterTarget::InventoryFiles),
                    app.is_filter_editing_target(FilterTarget::InventoryFiles),
                )),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut state = ListState::default().with_selected(
        filtered_inventory_indices
            .iter()
            .position(|idx| *idx == app.inventory_idx),
    );
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(5)])
        .split(chunks[1]);

    let (detail_cols, detail_spacing) = key_value_table_layout(right[0], 18);
    let details = Table::new(
        styled_key_value_rows(inventory_detail_rows(app)),
        detail_cols,
    )
    .column_spacing(detail_spacing)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(neutral_border_style())
            .style(Style::default().bg(th::BASE))
            .title("Inventory Details"),
    );
    frame.render_widget(details, right[0]);

    let preview = Paragraph::new(inventory_preview_text(
        app,
        right[1].height.saturating_sub(2) as usize,
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(neutral_border_style())
            .title("File Preview"),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(preview, right[1]);
}

fn render_inventory_hosts(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let Some(ref state) = app.inventory_edit_state else {
        let msg = Paragraph::new("No YAML inventory loaded. Select a YAML file and press 2.")
            .style(Style::default().fg(th::SUBTEXT0));
        frame.render_widget(msg, area);
        return;
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Host list (left panel)
    let list_items: Vec<ListItem> = if state.hosts.is_empty() {
        vec![ListItem::new("No hosts. Press n to add.")]
    } else {
        state
            .hosts
            .iter()
            .map(|h| ListItem::new(h.as_str()))
            .collect()
    };
    let dirty_marker = if state.dirty { " [*]" } else { "" };
    let focus_ctx = app.content_focus_context();
    let detail_focused = matches!(focus_ctx, FocusContext::InventoryHostDetails);
    let list_border = if matches!(focus_ctx, FocusContext::InventoryHostsList) {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let host_list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(list_border)
                .title(format!("Hosts{dirty_marker}")),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut list_state = ListState::default().with_selected(if state.hosts.is_empty() {
        None
    } else {
        Some(app.hosts_subtab_idx)
    });
    frame.render_stateful_widget(host_list, cols[0], &mut list_state);

    // Host detail (right panel)
    let detail_border = if detail_focused {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };

    if let Some(host) = state.hosts.get(app.hosts_subtab_idx) {
        let vars = state.host_vars.get(host);
        let mut rows: Vec<Row> = Vec::new();

        let fields: Vec<(&str, String)> = vec![
            (
                "ansible_host",
                vars.map(|v| v.ansible_host.clone()).unwrap_or_default(),
            ),
            (
                "ansible_user",
                vars.map(|v| v.ansible_user.clone()).unwrap_or_default(),
            ),
            (
                "ansible_port",
                vars.and_then(|v| v.ansible_port)
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
            ),
            (
                "ansible_connection",
                vars.map(|v| v.ansible_connection.clone())
                    .unwrap_or_default(),
            ),
        ];

        for (i, (key, value)) in fields.iter().enumerate() {
            let is_editing_field = app.hosts_subtab_editing
                && app.hosts_subtab_focus_detail
                && app.hosts_subtab_field_idx == i;
            let value_cell = if is_editing_field {
                Cell::from(format!("{}|", app.hosts_subtab_edit_buffer))
            } else if value.is_empty() {
                Cell::from(Line::from(Span::styled(
                    host_field_placeholder(*key),
                    Style::default()
                        .fg(th::SUBTEXT0)
                        .add_modifier(Modifier::ITALIC),
                )))
            } else {
                Cell::from(value.clone())
            };
            rows.push(Row::new(vec![Cell::from(*key), value_cell]));
        }

        if let Some(v) = vars {
            if !v.custom_vars.is_empty() {
                rows.push(Row::new(vec![
                    Cell::from("--- Custom ---").style(Style::default().fg(th::SUBTEXT0)),
                    Cell::from(""),
                ]));
            }
            for (ci, (key, value)) in v.custom_vars.iter().enumerate() {
                let field_i = 4 + ci;
                let is_editing_field = app.hosts_subtab_editing
                    && app.hosts_subtab_focus_detail
                    && app.hosts_subtab_field_idx == field_i;
                let value_cell = if is_editing_field {
                    Cell::from(format!("{}|", app.hosts_subtab_edit_buffer))
                } else if value.is_empty() {
                    Cell::from(Line::from(Span::styled(
                        "e.g. value",
                        Style::default()
                            .fg(th::SUBTEXT0)
                            .add_modifier(Modifier::ITALIC),
                    )))
                } else {
                    Cell::from(value.clone())
                };
                rows.push(Row::new(vec![Cell::from(key.as_str()), value_cell]));
            }
        }

        let detail_table = Table::new(rows, [Constraint::Length(20), Constraint::Min(10)])
            .header(
                Row::new(vec!["Property", "Value"]).style(
                    Style::default()
                        .fg(th::SUBTEXT1)
                        .add_modifier(Modifier::BOLD),
                ),
            )
            .column_spacing(1)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(detail_border)
                    .title(format!("Host: {host}")),
            )
            .row_highlight_style(if detail_focused {
                Style::default().fg(th::YELLOW)
            } else {
                Style::default()
            });
        let mut table_state = TableState::default().with_selected(if detail_focused {
            Some(if let Some(v) = vars {
                if !v.custom_vars.is_empty() && app.hosts_subtab_field_idx >= 4 {
                    // account for the separator row
                    app.hosts_subtab_field_idx + 1
                } else {
                    app.hosts_subtab_field_idx
                }
            } else {
                app.hosts_subtab_field_idx
            })
        } else {
            None
        });
        frame.render_stateful_widget(detail_table, cols[1], &mut table_state);
    } else {
        let empty = Paragraph::new("Select a host from the list")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(detail_border)
                    .title("Host Detail"),
            )
            .style(Style::default().fg(th::SUBTEXT0));
        frame.render_widget(empty, cols[1]);
    }

    // Input prompts
    if app.hosts_subtab_add_host_open {
        let prompt_area = centered_rect(50, 15, frame.area());
        frame.render_widget(Clear, prompt_area);
        let val = if app.hosts_subtab_add_host_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.hosts_subtab_add_host_buffer)
        };
        let input = Paragraph::new(val).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD))
                .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
                .title("New Host Name"),
        );
        frame.render_widget(input, prompt_area);
    }

    if app.hosts_subtab_add_var_open {
        let prompt_area = centered_rect(50, 15, frame.area());
        frame.render_widget(Clear, prompt_area);
        let val = if app.hosts_subtab_add_var_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.hosts_subtab_add_var_buffer)
        };
        let input = Paragraph::new(val).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD))
                .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
                .title("New Variable Name"),
        );
        frame.render_widget(input, prompt_area);
    }
}

fn host_field_placeholder(key: &str) -> &'static str {
    match key {
        "ansible_host" => "e.g. 192.0.2.10",
        "ansible_user" => "e.g. ubuntu",
        "ansible_port" => "e.g. 22",
        "ansible_connection" => "e.g. ssh",
        _ => "e.g. value",
    }
}

fn render_inventory_groups(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let Some(ref state) = app.inventory_edit_state else {
        let msg = Paragraph::new("No YAML inventory loaded. Select a YAML file and press 3.")
            .style(Style::default().fg(th::SUBTEXT0));
        frame.render_widget(msg, area);
        return;
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(35),
            Constraint::Percentage(35),
        ])
        .split(area);

    // Group tree (left) with box-drawing connectors
    let tree_nodes = app.groups_subtab_tree_nodes();
    let tree_items: Vec<ListItem> = if tree_nodes.is_empty() {
        vec![ListItem::new("No groups yet.")]
    } else {
        // Pre-compute: for each node, determine if it is the last sibling at its depth
        let mut is_last_at_depth: Vec<bool> = vec![false; tree_nodes.len()];
        for i in 0..tree_nodes.len() {
            let (_, depth) = &tree_nodes[i];
            let is_last = tree_nodes[i + 1..]
                .iter()
                .find(|(_, d)| *d <= *depth)
                .map(|(_, d)| *d < *depth)
                .unwrap_or(true);
            is_last_at_depth[i] = is_last;
        }
        // Track which ancestor depths still have more siblings
        let mut ancestors_open: Vec<bool> = Vec::new();
        tree_nodes
            .iter()
            .enumerate()
            .map(|(i, (group, depth))| {
                let label = match group {
                    None => String::from("all"),
                    Some(name) => {
                        if *depth == 0 {
                            name.clone()
                        } else {
                            // Adjust ancestors_open to match current depth
                            ancestors_open.truncate(depth.saturating_sub(1));
                            // Build prefix from ancestor continuation lines
                            let mut prefix = String::new();
                            for open in ancestors_open.iter() {
                                if *open {
                                    prefix.push_str("│  ");
                                } else {
                                    prefix.push_str("   ");
                                }
                            }
                            // Add connector for this node
                            if is_last_at_depth[i] {
                                prefix.push_str("└─ ");
                                ancestors_open.push(false);
                            } else {
                                prefix.push_str("├─ ");
                                ancestors_open.push(true);
                            }
                            format!("{prefix}{name}")
                        }
                    }
                };
                ListItem::new(label)
            })
            .collect()
    };
    let focus_ctx = app.content_focus_context();
    let tree_border = if matches!(focus_ctx, FocusContext::InventoryGroupsTree) {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let dirty_marker = if state.dirty { " [*]" } else { "" };
    let tree = List::new(tree_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(tree_border)
                .title(format!("Group Tree{dirty_marker}")),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut tree_state = ListState::default().with_selected(if tree_nodes.is_empty() {
        None
    } else {
        Some(app.groups_subtab_tree_idx)
    });
    frame.render_stateful_widget(tree, cols[0], &mut tree_state);

    let target_label = app.groups_subtab_target_group.as_deref().unwrap_or("all");

    // Groups list (middle)
    let candidate_groups = app.groups_subtab_candidate_groups();
    let group_items: Vec<ListItem> = if candidate_groups.is_empty() {
        vec![ListItem::new("No groups available.")]
    } else {
        candidate_groups
            .iter()
            .map(|group| {
                let attached = if let Some(target) = app.groups_subtab_target_group.as_ref() {
                    state
                        .group_children
                        .get(target)
                        .map(|c| c.contains(group))
                        .unwrap_or(false)
                } else {
                    !state.group_children.values().any(|c| c.contains(group))
                };
                if attached {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            "[x]",
                            Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!(" {group}")),
                    ]))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::styled("[ ]", Style::default().fg(th::SURFACE1)),
                        Span::styled(format!(" {group}"), Style::default().fg(th::SUBTEXT0)),
                    ]))
                }
            })
            .collect()
    };
    let groups_border = if matches!(focus_ctx, FocusContext::InventoryGroupsGroups) {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let groups_list = List::new(group_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(groups_border)
                .title(format!("Groups (target: {target_label})")),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut groups_state = ListState::default().with_selected(if candidate_groups.is_empty() {
        None
    } else {
        Some(app.groups_subtab_group_idx)
    });
    frame.render_stateful_widget(groups_list, cols[1], &mut groups_state);

    // Hosts list (right)
    let host_items: Vec<ListItem> = if state.hosts.is_empty() {
        vec![ListItem::new("No hosts. Press n to add.")]
    } else {
        state
            .hosts
            .iter()
            .map(|host| {
                let attached = if let Some(target) = app.groups_subtab_target_group.as_ref() {
                    state
                        .assignments
                        .get(target)
                        .map(|hosts| hosts.contains(host))
                        .unwrap_or(false)
                } else {
                    !state.assignments.values().any(|hosts| hosts.contains(host))
                };
                if attached {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            "[x]",
                            Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!(" {host}")),
                    ]))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::styled("[ ]", Style::default().fg(th::SURFACE1)),
                        Span::styled(format!(" {host}"), Style::default().fg(th::SUBTEXT0)),
                    ]))
                }
            })
            .collect()
    };
    let hosts_border = if matches!(focus_ctx, FocusContext::InventoryGroupsHosts) {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let hosts_list = List::new(host_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(hosts_border)
                .title("Hosts"),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut hosts_state = ListState::default().with_selected(if state.hosts.is_empty() {
        None
    } else {
        Some(app.groups_subtab_host_idx)
    });
    frame.render_stateful_widget(hosts_list, cols[2], &mut hosts_state);

    // Input prompts for groups sub-tab
    if app.hosts_subtab_add_var_open {
        let prompt_area = centered_rect(50, 15, frame.area());
        frame.render_widget(Clear, prompt_area);
        let val = if app.hosts_subtab_add_var_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.hosts_subtab_add_var_buffer)
        };
        let input = Paragraph::new(val).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD))
                .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
                .title("New Group Name"),
        );
        frame.render_widget(input, prompt_area);
    }

    if app.hosts_subtab_add_host_open {
        let prompt_area = centered_rect(50, 15, frame.area());
        frame.render_widget(Clear, prompt_area);
        let val = if app.hosts_subtab_add_host_buffer.is_empty() {
            String::from("|")
        } else {
            format!("{}|", app.hosts_subtab_add_host_buffer)
        };
        let input = Paragraph::new(val).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD))
                .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
                .title("New Host Name"),
        );
        frame.render_widget(input, prompt_area);
    }
}

fn render_playbooks(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
        vec![ListItem::new(
            "No playbooks found under ./playbooks or project root",
        )]
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
    let playbooks_border_style = if matches!(focus_ctx, FocusContext::PlaybooksList) {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let runs_border_style = if matches!(focus_ctx, FocusContext::PlaybooksRuns) {
        Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(playbooks_border_style)
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
                .border_style(runs_border_style)
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
                .border_style(neutral_border_style())
                .title("Playbook Settings"),
        );
        frame.render_widget(paragraph, chunks[1]);
    }
}

fn render_templates(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
        vec![ListItem::new("No templates found. Press n to create one.")]
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
                    Span::styled(
                        format!("({})", template.playbook),
                        Style::default().fg(th::SUBTEXT0),
                    ),
                ]))
            })
            .collect::<Vec<_>>()
    };

    let focus_ctx = app.content_focus_context();
    let template_list = List::new(template_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if matches!(focus_ctx, FocusContext::TemplatesList) {
                    Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                } else {
                    neutral_border_style()
                })
                .title(filtered_list_title(
                    "Templates",
                    app.filter_query_for(FilterTarget::Templates),
                    app.is_filter_editing_target(FilterTarget::Templates),
                )),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
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
    let run_list = List::new(template_run_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if matches!(focus_ctx, FocusContext::TemplatesRuns) {
                    Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD)
                } else {
                    neutral_border_style()
                })
                .title(filtered_list_title(
                    "Runs For Selected Template",
                    app.filter_query_for(FilterTarget::TemplateRuns),
                    app.is_filter_editing_target(FilterTarget::TemplateRuns),
                )),
        )
        .highlight_style(Style::default().fg(th::GREEN))
        .highlight_symbol(">> ");
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .style(Style::default().bg(th::BASE))
                .title("Template Settings"),
        );
    frame.render_widget(table, chunks[1]);
}

fn render_logs(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
                .border_style(border_style)
                .title(title),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
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

fn hint_line_from_bindings(bindings: &[HintBinding], max_items: usize) -> Line<'static> {
    Line::from(hint_spans_from_bindings(bindings, max_items))
}

fn hint_spans_from_bindings(bindings: &[HintBinding], max_items: usize) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (idx, hint) in bindings.iter().take(max_items).enumerate() {
        if idx > 0 {
            spans.push(Span::styled("  |  ", Style::default().fg(th::SUBTEXT1)));
        }
        spans.push(Span::styled(
            hint.key.to_string(),
            Style::default()
                .fg(th::HINT_KEY)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}", hint.desc),
            Style::default().fg(th::HINT_TEXT),
        ));
    }
    spans
}

fn hint_bar_style() -> Style {
    Style::default().fg(th::HINT_TEXT).bg(th::HINT_BG)
}

fn neutral_border_style() -> Style {
    Style::default()
        .fg(th::SURFACE0)
        .add_modifier(Modifier::DIM)
}

fn filtered_list_title(base: &str, query: &str, editing: bool) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        if editing {
            format!("{base} (/|)")
        } else {
            base.to_string()
        }
    } else if editing {
        format!("{base} (/{trimmed}|)")
    } else {
        format!("{base} (/{trimmed})")
    }
}

fn focus_context_label(context: FocusContext) -> &'static str {
    match context {
        FocusContext::RuntimePrompt => "runtime picker",
        FocusContext::Modal => "modal",
        FocusContext::Dashboard => "dashboard",
        FocusContext::Projects => "projects list",
        FocusContext::InventoryFiles => "inventory files",
        FocusContext::InventoryHostsList => "hosts list",
        FocusContext::InventoryHostDetails => "host details",
        FocusContext::InventoryGroupsTree => "groups tree",
        FocusContext::InventoryGroupsGroups => "candidate groups",
        FocusContext::InventoryGroupsHosts => "candidate hosts",
        FocusContext::PlaybooksList => "playbooks list",
        FocusContext::PlaybooksRuns => "playbook runs",
        FocusContext::PlaybooksLogSelect => "playbook logs",
        FocusContext::TemplatesList => "templates list",
        FocusContext::TemplatesRuns => "template runs",
        FocusContext::TemplatesLogSelect => "template logs",
        FocusContext::Settings => "settings",
    }
}

fn status_line_style(app: &App, runtime_required: bool) -> Style {
    if app.runtime_prompt_open && !app.runtime_bootstrapping && runtime_required {
        return Style::default().fg(th::CRUST).bg(th::RED);
    }
    let lowered = app.status_line.to_lowercase();
    if lowered.starts_with("error:")
        || lowered.contains(" failed")
        || lowered.contains("failed:")
        || lowered.contains("failed ")
    {
        return Style::default().fg(th::TEXT).bg(th::SURFACE0);
    }
    if lowered.contains("saved")
        || lowered.contains("updated")
        || lowered.contains("ready")
        || lowered.contains("started")
        || lowered.contains("succeeded")
    {
        return Style::default().fg(th::GREEN).bg(th::MANTLE);
    }
    Style::default().fg(th::TEXT).bg(th::MANTLE)
}

fn key_value_value_style(value: &str) -> Style {
    let lowered = value.trim().to_ascii_lowercase();
    if lowered == "true" {
        return Style::default().fg(th::GREEN);
    }
    if lowered == "false" {
        return Style::default().fg(th::SUBTEXT0);
    }
    if lowered == "unset" || lowered == "none" || lowered == "(none)" {
        return Style::default()
            .fg(th::SUBTEXT0)
            .add_modifier(Modifier::ITALIC);
    }
    Style::default().fg(th::TEXT)
}

fn styled_key_value_rows(rows: Vec<(String, String)>) -> Vec<Row<'static>> {
    rows.into_iter()
        .map(|(property, value)| {
            Row::new(vec![
                Cell::from(Line::from(Span::styled(
                    property,
                    Style::default().fg(th::SUBTEXT1),
                ))),
                Cell::from(Line::from(Span::styled(
                    value.clone(),
                    key_value_value_style(&value),
                ))),
            ])
        })
        .collect()
}

fn key_value_table_layout(
    area: ratatui::layout::Rect,
    preferred_key_width: u16,
) -> ([Constraint; 2], u16) {
    let width = area.width.saturating_sub(2);
    if width <= 46 {
        ([Constraint::Percentage(44), Constraint::Percentage(56)], 1)
    } else if width <= 62 {
        (
            [
                Constraint::Length(preferred_key_width.saturating_sub(8).max(14)),
                Constraint::Min(10),
            ],
            1,
        )
    } else if width <= 84 {
        (
            [
                Constraint::Length(preferred_key_width.saturating_sub(4).max(16)),
                Constraint::Min(10),
            ],
            1,
        )
    } else {
        (
            [Constraint::Length(preferred_key_width), Constraint::Min(10)],
            2,
        )
    }
}

fn render_settings(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(8)])
        .split(area);

    let mode_line = if app.global_settings_text_mode {
        Paragraph::new("Global settings edit mode: ON").style(
            Style::default()
                .fg(th::CRUST)
                .bg(th::YELLOW)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Paragraph::new("Global settings edit mode: OFF").style(Style::default().fg(th::SUBTEXT1))
    };
    frame.render_widget(mode_line, chunks[0]);

    let (detail_cols, detail_spacing) = key_value_table_layout(chunks[1], 30);
    let table = Table::new(
        styled_key_value_rows(global_settings_rows(app)),
        detail_cols,
    )
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
            .border_style(neutral_border_style())
            .style(Style::default().bg(th::BASE))
            .title("Global Settings"),
    );
    let mut state = TableState::default().with_selected(Some(app.global_settings_field_idx));
    frame.render_stateful_widget(table, chunks[1], &mut state);
}

fn global_settings_rows(app: &App) -> Vec<(String, String)> {
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

fn display_global_settings_text(app: &App, idx: usize, current: &str) -> String {
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

fn render_runtime_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(88, 78, frame.area());
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(9),
            Constraint::Length(1),
            Constraint::Min(4),
        ])
        .split(area);

    let runtime_required = !playbook_bin_available(&app.run_options.ansible_bin);
    let alert_style = if app.runtime_bootstrapping {
        Style::default().fg(th::CRUST).bg(th::YELLOW)
    } else if runtime_required {
        Style::default()
            .fg(th::CRUST)
            .bg(th::RED)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(th::CRUST)
            .bg(th::GREEN)
            .add_modifier(Modifier::BOLD)
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
        Span::styled(
            " Runtime Setup (bootstrapping) ",
            Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD),
        )
    } else if runtime_required {
        Span::styled(
            " Runtime Setup Required ",
            Style::default().fg(th::RED).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " Runtime Selector ",
            Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD),
        )
    };
    let header = Paragraph::new(
        "Select an existing Ansible runtime below, or press b to install a managed runtime in ./.ansible-tui/runtime.",
    )
        .block(Block::default().borders(Borders::ALL).title(title))
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Candidates"),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Setup Logs"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(logs, chunks[4]);
}

fn render_project_create_prompt(frame: &mut Frame, app: &App) {
    let is_git = app.project_create_mode == ProjectCreateMode::Git;
    let area = centered_rect(72, if is_git { 62 } else { 56 }, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title(app.project_create_mode.title());
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
    frame.render_widget(
        Paragraph::new(intro).style(Style::default().fg(th::SUBTEXT1)),
        chunks[0],
    );

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
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(if focused {
                Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
            } else {
                neutral_border_style()
            })
            .title(*label);
        frame.render_widget(
            Paragraph::new(display)
                .block(block)
                .style(Style::default().fg(th::TEXT).bg(th::BASE)),
            chunks[idx + 1],
        );
    }

    frame.render_widget(
        Paragraph::new(hint_line_from_bindings(&HINTS_PROJECT_CREATE, 6)).style(hint_bar_style()),
        chunks[chunks.len() - 1],
    );
}

fn render_project_ssh_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(78, 72, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Project Secret Settings");
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
            Constraint::Length(2),
        ])
        .split(inner);

    let project_name = app
        .project_ssh_target_name()
        .unwrap_or_else(|| String::from("(unknown)"));
    frame.render_widget(
        Paragraph::new(format!(
            "Project: {project_name} | Vault refs default from project and can be overridden in templates."
        ))
        .style(Style::default().fg(th::SUBTEXT1)),
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if file_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("SSH Key File Path"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if inline_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Inline SSH Private Key"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE))
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if source_focused {
                    Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                } else {
                    neutral_border_style()
                })
                .title("Vault Source Type"),
        )
        .style(Style::default().fg(th::TEXT).bg(th::BASE)),
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if vault_file_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Vault Password File"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(if vault_id_focused {
                        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                    } else {
                        neutral_border_style()
                    })
                    .title("Vault ID Label"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE)),
        chunks[5],
    );

    frame.render_widget(
        Paragraph::new(hint_line_from_bindings(&HINTS_PROJECT_SECRETS, 6)).style(hint_bar_style()),
        chunks[6],
    );
}

fn render_vault_create_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(82, 74, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
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

fn render_vault_password_create_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(72, 44, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
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

fn render_vault_edit_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(84, 78, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
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

fn render_vault_runtime_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(68, 34, frame.area());
    frame.render_widget(Clear, area);
    let confirm_required = app.vault_runtime_prompt_confirm_required();

    let wrapper = Block::default()
        .borders(Borders::ALL)
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

fn render_inventory_create_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(54, 26, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Create Inventory");
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
            Constraint::Length(1),
        ])
        .split(inner);

    let intro = Paragraph::new("Create under ./inventory (.ini, .yml, .yaml).")
        .style(Style::default().fg(th::SUBTEXT1));
    frame.render_widget(intro, chunks[0]);

    let input_value = if app.inventory_create_buffer.is_empty() {
        String::from("|")
    } else {
        format!("{}|", app.inventory_create_buffer)
    };
    let input = Paragraph::new(input_value)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th::YELLOW))
                .title("Filename"),
        )
        .style(Style::default().fg(th::TEXT).bg(th::BASE));
    frame.render_widget(input, chunks[1]);

    let hint =
        Paragraph::new(hint_line_from_bindings(&HINTS_INVENTORY_CREATE, 4)).style(hint_bar_style());
    frame.render_widget(hint, chunks[2]);
}

fn render_inventory_edit_mode_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(56, 30, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Edit Inventory Mode");
    frame.render_widget(wrapper, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(5),
            Constraint::Length(2),
        ])
        .split(inner);

    let selected = app
        .inventories
        .get(app.inventory_idx)
        .map(|path| display_path(app.active_project_root(), path))
        .unwrap_or_else(|| String::from("(none)"));
    let intro =
        Paragraph::new(format!("Inventory: {selected}")).style(Style::default().fg(th::SUBTEXT1));
    frame.render_widget(intro, chunks[0]);

    let items = vec![
        ListItem::new("External Editor ($VISUAL/$EDITOR/vim)"),
        ListItem::new("Built-in Text Editor (raw YAML/INI)"),
    ];
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Choose Mode"),
        )
        .highlight_style(Style::default().fg(th::CRUST).bg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut state = ListState::default().with_selected(Some(app.inventory_edit_mode_idx));
    frame.render_stateful_widget(list, chunks[1], &mut state);

    let hint = Paragraph::new(hint_line_from_bindings(&HINTS_INVENTORY_EDIT_MODE, 6))
        .style(hint_bar_style());
    frame.render_widget(hint, chunks[2]);
}

fn render_inventory_editor(frame: &mut Frame, app: &App) {
    let area = centered_rect(84, 82, frame.area());
    frame.render_widget(Clear, area);

    let border_color = if app.inventory_editor_dirty {
        th::YELLOW
    } else {
        th::MAUVE
    };
    let title = if app.inventory_editor_dirty {
        "Inventory Editor (unsaved)"
    } else {
        "Inventory Editor"
    };
    let wrapper = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title(title);
    frame.render_widget(wrapper, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(inner);

    let file_label = app
        .inventory_editor_path
        .as_ref()
        .map(|p| display_path(app.active_project_root(), p))
        .unwrap_or_else(|| String::from("(none)"));
    let file_info = Paragraph::new(format!("File: {file_label}"))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style()),
        )
        .style(Style::default().fg(th::TEXT).bg(th::BASE));
    frame.render_widget(file_info, chunks[0]);

    let body = if app.inventory_editor_buffer.is_empty() {
        Text::from("|")
    } else {
        Text::from(format!("{}|", app.inventory_editor_buffer))
    };
    let editor = Paragraph::new(body)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Content"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(editor, chunks[1]);

    let hint =
        Paragraph::new(hint_line_from_bindings(&HINTS_INVENTORY_EDITOR, 6)).style(hint_bar_style());
    frame.render_widget(hint, chunks[2]);
}

fn render_playbook_settings_editor(frame: &mut Frame, app: &App) {
    let area = centered_rect(76, 72, frame.area());
    frame.render_widget(Clear, area);

    let border_color = if app.settings_editor_text_mode {
        th::YELLOW
    } else {
        th::MAUVE
    };
    let wrapper = Block::default()
        .borders(Borders::ALL)
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

fn render_template_editor(frame: &mut Frame, app: &App) {
    let area = centered_rect(86, 84, frame.area());
    frame.render_widget(Clear, area);

    let border_color = if app.template_editor_text_mode {
        th::YELLOW
    } else {
        th::MAUVE
    };
    let title = if app.template_editor_editing_id.is_some() {
        "Template Editor (Edit)"
    } else {
        "Template Editor (New)"
    };
    let wrapper = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title(title);
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
            Constraint::Length(2),
        ])
        .split(inner);

    let mode_label = if app.template_editor_text_mode {
        "Edit mode: ON"
    } else {
        "Edit mode: OFF"
    };
    let mode_style = if app.template_editor_text_mode {
        Style::default()
            .fg(th::CRUST)
            .bg(th::YELLOW)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th::SUBTEXT1)
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
            .border_style(neutral_border_style()),
    )
    .style(Style::default().fg(th::TEXT).bg(th::BASE));
    frame.render_widget(header, chunks[1]);

    let rows = template_editor_rows(app);
    let (detail_cols, detail_spacing) = key_value_table_layout(chunks[2], 34);
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
                .border_style(neutral_border_style())
                .style(Style::default().fg(th::TEXT).bg(th::BASE))
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

fn template_editor_rows(app: &App) -> Vec<(String, String)> {
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

fn settings_rows(app: &App, settings: &PlaybookSettings) -> Vec<(String, String)> {
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

fn inventory_detail_rows(app: &App) -> Vec<(String, String)> {
    let Some(path) = app.inventories.get(app.inventory_idx) else {
        return vec![
            (String::from("selected"), String::from("none")),
            (String::from("path"), String::from("unset")),
            (String::from("format"), String::from("unset")),
            (String::from("size"), String::from("unset")),
            (String::from("lines"), String::from("unset")),
            (String::from("writable"), String::from("unset")),
        ];
    };

    let display = display_path(app.active_project_root(), path);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown")
        .to_string();
    let metadata = fs::metadata(path).ok();
    let size = metadata
        .as_ref()
        .map(|m| format!("{} B", m.len()))
        .unwrap_or_else(|| String::from("unavailable"));
    let writable = metadata
        .as_ref()
        .map(|m| (!m.permissions().readonly()).to_string())
        .unwrap_or_else(|| String::from("unavailable"));
    let lines = fs::read_to_string(path)
        .map(|content| content.lines().count().to_string())
        .unwrap_or_else(|_| String::from("unavailable"));

    vec![
        (String::from("selected"), display.clone()),
        (String::from("path"), display),
        (String::from("format"), ext),
        (String::from("size"), size),
        (String::from("lines"), lines),
        (String::from("writable"), writable),
    ]
}

fn inventory_preview_text(app: &App, max_lines: usize) -> Text<'static> {
    let Some(path) = app.inventories.get(app.inventory_idx) else {
        return Text::from("No inventory selected.");
    };

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            return Text::from(format!("Failed to read inventory: {err}"));
        }
    };
    if content.trim().is_empty() {
        return Text::from("Inventory file is empty.");
    }

    let cap = max_lines.max(1);
    let all_lines = content.lines().collect::<Vec<_>>();
    let mut lines = all_lines
        .iter()
        .take(cap)
        .map(|line| {
            if line.trim_start().starts_with('#') {
                Line::styled((*line).to_string(), Style::default().fg(th::SUBTEXT0))
            } else {
                Line::raw((*line).to_string())
            }
        })
        .collect::<Vec<_>>();

    if all_lines.len() > cap {
        lines.push(Line::styled(
            format!("... {} more lines", all_lines.len() - cap),
            Style::default().fg(th::SUBTEXT0),
        ));
    }
    Text::from(lines)
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

fn settings_preview_rows(
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

fn template_settings_preview_rows(
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

fn display_setting_text(app: &App, idx: usize, current: &str) -> String {
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

fn display_template_editor_text(app: &App, idx: usize, current: &str) -> String {
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

fn display_setting_inline_key_text(app: &App, idx: usize, current: Option<&str>) -> String {
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

fn display_template_editor_inline_key_text(app: &App, idx: usize, current: Option<&str>) -> String {
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

fn summarize_inline_key(value: Option<&str>) -> String {
    match value {
        Some(value) if !value.trim().is_empty() => {
            format!(
                "set ({} lines, {} chars)",
                value.lines().count(),
                value.chars().count()
            )
        }
        _ => String::from("unset"),
    }
}

fn masked_secret(value: &str, focused: bool) -> String {
    let stars = "*".repeat(value.chars().count().max(1));
    if focused {
        format!("{stars}|")
    } else {
        stars
    }
}

fn dashboard_stat_card(
    label: &str,
    value: String,
    value_color: ratatui::style::Color,
) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::styled(
            value,
            Style::default()
                .fg(value_color)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(label.to_string(), Style::default().fg(th::SUBTEXT1)),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(neutral_border_style()),
    )
}

fn top_ranked_items(counts: &BTreeMap<String, u64>, max_items: usize) -> (Vec<(String, u64)>, u64) {
    let mut items = counts
        .iter()
        .map(|(name, count)| (name.clone(), *count))
        .collect::<Vec<_>>();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(max_items);

    let max_value = items.iter().map(|(_, count)| *count).max().unwrap_or(1);
    (items, max_value)
}

fn last_path_segment(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}

fn render_ranked_bar_panel(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    title: &str,
    items: &[(String, u64)],
    max_value: u64,
    bar_color: ratatui::style::Color,
) {
    let bar_width = area.width.saturating_sub(8) as usize;
    let mut lines = Vec::new();

    if items.is_empty() {
        lines.push(Line::styled(
            "No run data yet.",
            Style::default().fg(th::SUBTEXT0),
        ));
    } else {
        for (name, count) in items {
            lines.push(Line::styled(
                last_path_segment(name).to_string(),
                Style::default().fg(th::SUBTEXT1),
            ));

            let ratio = if max_value == 0 {
                0.0
            } else {
                *count as f64 / max_value as f64
            };
            let fill = if bar_width == 0 {
                0
            } else {
                ((ratio * bar_width as f64).round() as usize).max(1)
            };
            let bar = "█".repeat(fill);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:>3} ", count),
                    Style::default().fg(th::TEXT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(bar, Style::default().fg(bar_color)),
            ]));
        }
    }

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title(title),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

fn render_project_summary_panel(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    app: &App,
    running: usize,
    succeeded: usize,
    failed: usize,
    total_runs: usize,
) {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "Project: ",
            Style::default()
                .fg(th::SUBTEXT1)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(app.active_project_name(), Style::default().fg(th::TEXT)),
    ]));
    lines.push(Line::styled(
        format!("Total runs: {total_runs}"),
        Style::default().fg(th::SUBTEXT1),
    ));
    lines.push(Line::styled(
        format!("Succeeded: {succeeded}"),
        Style::default().fg(th::GREEN),
    ));
    lines.push(Line::styled(
        format!("Failed: {failed}"),
        Style::default().fg(th::RED),
    ));
    lines.push(Line::styled(
        format!("Running: {running}"),
        Style::default().fg(th::YELLOW),
    ));

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(neutral_border_style())
                .title("Project Summary"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

fn log_line_style(line: &str) -> Style {
    if line.starts_with("TASK [") {
        return Style::default().fg(th::MAUVE).add_modifier(Modifier::BOLD);
    }
    if line.starts_with("PLAY [") || line.starts_with("PLAY RECAP") {
        return Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD);
    }
    if line.starts_with("$ ") {
        return Style::default().fg(th::SUBTEXT1);
    }
    if line.starts_with("ok:") || (line.contains("failed=0") && line.contains("changed=0")) {
        return Style::default().fg(th::GREEN);
    }
    if line.starts_with("changed:")
        || (line.contains("changed=") && has_nonzero_metric(line, "changed="))
    {
        return Style::default().fg(th::YELLOW);
    }
    if line.starts_with("skipping:") {
        return Style::default().fg(th::SUBTEXT0);
    }
    if line.contains("[stderr]")
        || line.contains("FAILED!")
        || line.starts_with("fatal:")
        || has_nonzero_metric(line, "failed=")
        || has_nonzero_metric(line, "unreachable=")
    {
        return Style::default().fg(th::RED).add_modifier(Modifier::BOLD);
    }
    Style::default().fg(th::TEXT)
}

fn has_nonzero_metric(line: &str, key: &str) -> bool {
    if let Some(pos) = line.find(key) {
        let value = line[pos + key.len()..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>();
        return value.parse::<u64>().map(|v| v > 0).unwrap_or(false);
    }
    false
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    area: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
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

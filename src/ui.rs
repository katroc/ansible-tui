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
    display_path, App, GroupsFocus, InventorySubTab, ProjectCreateMode, RunStatus, View,
};
use crate::playbook_settings::PlaybookSettings;
use crate::run::playbook_bin_available;
use crate::theme as th;

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
    if app.inventory_edit_mode_open {
        render_inventory_edit_mode_prompt(frame, app);
    }
    if app.inventory_editor_open {
        render_inventory_editor(frame, app);
    }
    if app.runtime_prompt_open {
        render_runtime_prompt(frame, app);
    }
}

fn render_body(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.current_view() == View::Dashboard || app.current_view() == View::Projects {
        render_main(frame, app, area);
        return;
    }

    if app.current_view() == View::Inventory
        && !matches!(app.inventory_sub_tab, InventorySubTab::Files)
    {
        render_main(frame, app, area);
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
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
                .border_style(Style::default().fg(th::SURFACE1))
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
    let playbooks_with_runs = app
        .runs
        .iter()
        .map(|run| run.playbook.clone())
        .collect::<HashSet<_>>()
        .len();
    let coverage_percent: u16 = if app.playbooks.is_empty() {
        0
    } else {
        ((playbooks_with_runs as f64 / app.playbooks.len() as f64) * 100.0).round() as u16
    };

    let mut runs_by_playbook = BTreeMap::<String, u64>::new();
    let mut runs_by_inventory = BTreeMap::<String, u64>::new();
    let mut runs_by_environment = BTreeMap::<String, u64>::new();
    let mut successes_by_environment = BTreeMap::<String, u64>::new();
    for run in &app.runs {
        *runs_by_playbook.entry(run.playbook.clone()).or_insert(0) += 1;
        *runs_by_inventory.entry(run.inventory.clone()).or_insert(0) += 1;
        if let Some(environment) = run.environment.as_deref() {
            *runs_by_environment
                .entry(environment.to_string())
                .or_insert(0) += 1;
            if matches!(run.status, RunStatus::Succeeded) {
                *successes_by_environment
                    .entry(environment.to_string())
                    .or_insert(0) += 1;
            }
        }
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
                .border_style(Style::default().fg(th::SURFACE1))
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
                .border_style(Style::default().fg(th::SURFACE1))
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
                .border_style(Style::default().fg(th::SURFACE1))
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
                .border_style(Style::default().fg(th::SURFACE1))
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
                .border_style(Style::default().fg(th::SURFACE1))
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
                .border_style(Style::default().fg(th::SURFACE1))
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
    render_environment_breakdown_panel(
        frame,
        right[2],
        &runs_by_environment,
        &successes_by_environment,
    );
}

fn render_projects(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);

    let items = if app.projects.is_empty() {
        vec![ListItem::new("No projects configured.")]
    } else {
        app.projects
            .iter()
            .enumerate()
            .map(|(idx, project)| {
                let active = if idx == app.active_project_idx {
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
                .border_style(Style::default().fg(th::SURFACE1))
                .title("Projects"),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut state = ListState::default().with_selected(if app.projects.is_empty() {
        None
    } else {
        Some(app.project_idx)
    });
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
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
            (String::from("playbooks"), String::from("0")),
            (String::from("inventories"), String::from("0")),
        ]
    };
    let table_rows = rows
        .into_iter()
        .map(|(property, value)| Row::new(vec![Cell::from(property), Cell::from(value)]))
        .collect::<Vec<_>>();
    let details = Table::new(table_rows, [Constraint::Length(18), Constraint::Min(10)])
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
                .border_style(Style::default().fg(th::SURFACE1))
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
                    Style::default().fg(th::SURFACE1)
                })
                .title("Project Sync Logs"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(logs, right[1]);

    let hint = Paragraph::new(
        "n new | f import path | g clone git | e ssh key settings | a/Enter activate | Shift+D delete | i inventory sync | v vars sync | j/k select",
    )
    .style(Style::default().fg(th::SUBTEXT0));
    frame.render_widget(hint, right[2]);
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

    let items = if app.inventories.is_empty() {
        vec![ListItem::new("No inventories found under ./inventory")]
    } else {
        app.inventories
            .iter()
            .map(|p| ListItem::new(display_path(app.active_project_root(), p)))
            .collect::<Vec<_>>()
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th::SURFACE1))
                .title("Inventories"),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut state = ListState::default().with_selected(if app.inventories.is_empty() {
        None
    } else {
        Some(app.inventory_idx)
    });
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),
            Constraint::Min(5),
            Constraint::Length(2),
        ])
        .split(chunks[1]);

    let rows = inventory_detail_rows(app)
        .into_iter()
        .map(|(property, value)| Row::new(vec![Cell::from(property), Cell::from(value)]))
        .collect::<Vec<_>>();
    let details = Table::new(rows, [Constraint::Length(16), Constraint::Min(10)])
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
                .border_style(Style::default().fg(th::SURFACE1))
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
            .border_style(Style::default().fg(th::SURFACE1))
            .title("File Preview"),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(preview, right[1]);

    let hint = Paragraph::new(
        "n new inventory | e edit selected (choose mode) | Shift+D delete selected | j/k select | 2 Hosts | 3 Groups",
    )
    .style(Style::default().fg(th::SUBTEXT0))
    .wrap(Wrap { trim: true });
    frame.render_widget(hint, right[2]);
}

fn render_inventory_hosts(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let Some(ref state) = app.inventory_edit_state else {
        let msg = Paragraph::new("No YAML inventory loaded. Select a YAML file and press 2.")
            .style(Style::default().fg(th::SUBTEXT0));
        frame.render_widget(msg, area);
        return;
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(2)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(layout[0]);

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
    let list_border = if !app.hosts_subtab_focus_detail {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th::SURFACE1)
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
    let detail_border = if app.hosts_subtab_focus_detail {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th::SURFACE1)
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
            let display_val = if app.hosts_subtab_editing
                && app.hosts_subtab_focus_detail
                && app.hosts_subtab_field_idx == i
            {
                format!("{}|", app.hosts_subtab_edit_buffer)
            } else {
                value.clone()
            };
            rows.push(Row::new(vec![Cell::from(*key), Cell::from(display_val)]));
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
                let display_val = if app.hosts_subtab_editing
                    && app.hosts_subtab_focus_detail
                    && app.hosts_subtab_field_idx == field_i
                {
                    format!("{}|", app.hosts_subtab_edit_buffer)
                } else {
                    value.clone()
                };
                rows.push(Row::new(vec![
                    Cell::from(key.as_str()),
                    Cell::from(display_val),
                ]));
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
            .row_highlight_style(if app.hosts_subtab_focus_detail {
                Style::default().fg(th::YELLOW)
            } else {
                Style::default()
            });
        let mut table_state =
            TableState::default().with_selected(if app.hosts_subtab_focus_detail {
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

    let hint = Paragraph::new(
        "h/l focus | j/k nav | e edit | n add host | a add var | D delete | Ctrl+S save | Esc back",
    )
    .style(Style::default().fg(th::SUBTEXT0))
    .wrap(Wrap { trim: true });
    frame.render_widget(hint, layout[1]);
}

fn render_inventory_groups(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let Some(ref state) = app.inventory_edit_state else {
        let msg = Paragraph::new("No YAML inventory loaded. Select a YAML file and press 3.")
            .style(Style::default().fg(th::SUBTEXT0));
        frame.render_widget(msg, area);
        return;
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(2)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(35),
            Constraint::Percentage(35),
        ])
        .split(layout[0]);

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
    let tree_border = if app.groups_subtab_focus == GroupsFocus::Tree {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th::SURFACE1)
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
    let groups_border = if app.groups_subtab_focus == GroupsFocus::Groups {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th::SURFACE1)
    };
    let groups_list = List::new(group_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(groups_border)
                .title("Groups"),
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
    let hosts_border = if app.groups_subtab_focus == GroupsFocus::Hosts {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(th::SURFACE1)
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

    let target_label = app.groups_subtab_target_group.as_deref().unwrap_or("all");
    let hint = Paragraph::new(format!(
        "Target: {target_label} | h/l focus | j/k nav | Space toggle | n add | d detach | D delete | Ctrl+S save | Esc back",
    ))
    .style(Style::default().fg(th::SUBTEXT0))
    .wrap(Wrap { trim: true });
    frame.render_widget(hint, layout[1]);
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

    let items = if app.playbooks.is_empty() {
        vec![ListItem::new(
            "No playbooks found under ./playbooks or project root",
        )]
    } else {
        app.playbooks
            .iter()
            .map(|p| ListItem::new(display_path(app.active_project_root(), p)))
            .collect::<Vec<_>>()
    };
    let playbooks_border_style = if app.playbooks_focus_runs {
        Style::default().fg(th::SURFACE1)
    } else {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(playbooks_border_style)
                .title("Playbooks"),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut state = ListState::default().with_selected(if app.playbooks.is_empty() {
        None
    } else {
        Some(app.playbook_idx)
    });
    frame.render_stateful_widget(list, top[0], &mut state);

    let run_indices = app.run_indices_for_selected_playbook();
    let run_items = if run_indices.is_empty() {
        vec![ListItem::new(
            "No runs yet for this playbook. Press r to run.",
        )]
    } else {
        run_indices
            .iter()
            .map(|idx| {
                let run = &app.runs[*idx];
                let code = run
                    .exit_code
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| String::from("-"));
                let mut spans = vec![Span::raw(format!(
                    "#{:03} {:>9} code:{:<4} {}",
                    run.id,
                    run.status.as_str(),
                    code,
                    run.started_at.format("%H:%M:%S")
                ))];
                if let Some(env) = run.environment.as_deref() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        format!("[{}]", env.to_uppercase()),
                        Style::default()
                            .fg(th::SUBTEXT0)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect::<Vec<_>>()
    };
    let runs = List::new(run_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if app.playbooks_focus_runs {
                    Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(th::SURFACE1)
                })
                .title("Runs For Selected Playbook"),
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
        let bottom_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(1)])
            .split(chunks[1]);
        let rows = settings_preview_rows(&selected, &selected_inventory, &settings)
            .into_iter()
            .map(|(property, value)| Row::new(vec![Cell::from(property), Cell::from(value)]))
            .collect::<Vec<_>>();
        let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(10)])
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
                    .border_style(Style::default().fg(th::SURFACE1))
                    .title("Playbook Settings"),
            );
        frame.render_widget(table, bottom_chunks[0]);

        frame.render_widget(
            Paragraph::new(
                "Press t to edit settings | i/I cycle inventory target | PgUp/PgDn scroll logs | End follow latest | <-/-> focus Playbooks/Runs | Up/Down move focused list",
            )
            .style(Style::default().fg(th::SUBTEXT0)),
            bottom_chunks[1],
        );
    } else {
        let paragraph = Paragraph::new(vec![
            Line::raw("No playbook selected"),
            Line::raw("Press t in Playbooks tab to create/edit settings"),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th::SURFACE1))
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
    let template_items = if filtered_template_indices.is_empty() {
        vec![ListItem::new("No templates found. Press n to create one.")]
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

    let template_list = List::new(template_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if app.templates_focus_runs {
                    Style::default().fg(th::SURFACE1)
                } else {
                    Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
                })
                .title("Templates"),
        )
        .highlight_style(Style::default().fg(th::YELLOW))
        .highlight_symbol(">> ");
    let selected_template_pos = filtered_template_indices
        .iter()
        .position(|idx| *idx == app.template_idx);
    let mut template_state = ListState::default().with_selected(selected_template_pos);
    frame.render_stateful_widget(template_list, top[0], &mut template_state);

    let template_run_indices = app.run_indices_for_selected_template();
    let template_run_items = if template_run_indices.is_empty() {
        vec![ListItem::new(
            "No runs yet for this template. Press r to run.",
        )]
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
                .border_style(if app.templates_focus_runs {
                    Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(th::SURFACE1)
                })
                .title("Runs For Selected Template"),
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
    let bottom_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(1)])
        .split(chunks[1]);

    let table = Table::new(
        rows.into_iter()
            .map(|(k, v)| Row::new(vec![Cell::from(k), Cell::from(v)]))
            .collect::<Vec<_>>(),
        [Constraint::Length(20), Constraint::Min(10)],
    )
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
            .border_style(Style::default().fg(th::SURFACE1))
            .title("Template Settings"),
    );
    frame.render_widget(table, bottom_chunks[0]);

    let hint = Paragraph::new(
        "r run | t/e edit template | n new | Shift+D delete | <-/-> focus templates/runs",
    )
    .style(Style::default().fg(th::SUBTEXT0))
    .wrap(Wrap { trim: true });
    frame.render_widget(hint, bottom_chunks[1]);
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

    let (title, border_style) = if app.current_view() == View::Templates {
        (
            "Template Run Logs",
            if app.templates_focus_runs {
                Style::default().fg(th::GREEN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(th::SURFACE1)
            },
        )
    } else {
        (
            "Live Logs (Selected Run)",
            Style::default().fg(th::SURFACE1),
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
    let style = if app.runtime_prompt_open && !app.runtime_bootstrapping && runtime_required {
        Style::default().fg(th::CRUST).bg(th::RED)
    } else {
        Style::default().fg(th::TEXT).bg(th::MANTLE)
    };
    let status = Paragraph::new(app.status_line.clone()).style(style);
    frame.render_widget(status, area);
}

fn render_help(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let helper = Paragraph::new(active_help_text(app))
        .style(Style::default().fg(th::TEXT).bg(th::SURFACE1))
        .wrap(Wrap { trim: true });
    frame.render_widget(helper, area);
}

fn render_settings(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
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

    let rows = global_settings_rows(app)
        .into_iter()
        .map(|(property, value)| Row::new(vec![Cell::from(property), Cell::from(value)]))
        .collect::<Vec<_>>();
    let table = Table::new(rows, [Constraint::Length(28), Constraint::Min(10)])
        .header(
            Row::new(vec!["Property", "Value"]).style(
                Style::default()
                    .fg(th::SUBTEXT1)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .column_spacing(1)
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
                .border_style(Style::default().fg(th::SURFACE1))
                .title("Global Settings"),
        );
    let mut state = TableState::default().with_selected(Some(app.global_settings_field_idx));
    frame.render_stateful_widget(table, chunks[1], &mut state);

    let hints = Paragraph::new(
        "j/k field | space toggle bool | h/l or <-/-> adjust | Enter or e edit text | saved to ansible.cfg | u runtime picker",
    )
    .style(Style::default().fg(th::SUBTEXT0));
    frame.render_widget(hints, chunks[2]);
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
                .border_style(Style::default().fg(th::SURFACE1))
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

    let hints = Paragraph::new(
        "Keys: j/k move  Enter select runtime  b bootstrap managed runtime  Esc close",
    )
    .style(Style::default().fg(th::SUBTEXT0));
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
                .border_style(Style::default().fg(th::SURFACE1))
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
                Style::default().fg(th::SURFACE1)
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
        Paragraph::new(
            "Type value | Up/Down field | Enter next/save | Backspace edit | Esc cancel",
        )
        .style(Style::default().fg(th::SUBTEXT0)),
        chunks[chunks.len() - 1],
    );
}

fn render_project_ssh_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(74, 64, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(th::MAUVE))
        .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
        .title("Project SSH Key Settings");
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
            Constraint::Length(2),
        ])
        .split(inner);

    let project_name = app
        .project_ssh_target_name()
        .unwrap_or_else(|| String::from("(unknown)"));
    frame.render_widget(
        Paragraph::new(format!(
            "Project: {project_name} | Inline key overrides file path at project scope."
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
                        Style::default().fg(th::SURFACE1)
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
                        Style::default().fg(th::SURFACE1)
                    })
                    .title("Inline SSH Private Key"),
            )
            .style(Style::default().fg(th::TEXT).bg(th::BASE))
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    frame.render_widget(
        Paragraph::new(
            "Type text | Up/Down field | Enter next/newline | Ctrl+S save | Backspace edit | Esc cancel",
        )
        .style(Style::default().fg(th::SUBTEXT0)),
        chunks[3],
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

    let hint = Paragraph::new("Enter create | Backspace edit | Esc cancel")
        .style(Style::default().fg(th::SUBTEXT0));
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
                .border_style(Style::default().fg(th::SURFACE1))
                .title("Choose Mode"),
        )
        .highlight_style(Style::default().fg(th::CRUST).bg(th::YELLOW))
        .highlight_symbol(">> ");
    let mut state = ListState::default().with_selected(Some(app.inventory_edit_mode_idx));
    frame.render_stateful_widget(list, chunks[1], &mut state);

    let hint =
        Paragraph::new("Enter confirm | j/k or Up/Down select | 1/2 quick select | e external | t text | Esc cancel")
            .style(Style::default().fg(th::SUBTEXT0));
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
                .border_style(Style::default().fg(th::SURFACE1)),
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
                .border_style(Style::default().fg(th::SURFACE1))
                .title("Content"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(editor, chunks[1]);

    let hint =
        Paragraph::new("Type to edit | Enter newline | Backspace delete | Ctrl+S save | Esc close")
            .style(Style::default().fg(th::SUBTEXT0));
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
                .border_style(Style::default().fg(th::SURFACE1)),
        )
        .style(Style::default().fg(th::TEXT).bg(th::BASE));
    frame.render_widget(header, chunks[1]);

    let settings = app.selected_playbook_settings().unwrap_or_default();
    let rows = settings_rows(app, &settings);
    let table_rows = rows
        .iter()
        .enumerate()
        .map(|(idx, (property, value))| {
            let style = if idx == app.settings_editor_field_idx {
                Style::default()
                    .fg(th::CRUST)
                    .bg(th::YELLOW)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(th::TEXT)
            };
            Row::new(vec![
                Cell::from(property.clone()),
                Cell::from(value.clone()),
            ])
            .style(style)
        })
        .collect::<Vec<_>>();
    let fields = Table::new(table_rows, [Constraint::Length(28), Constraint::Min(10)])
        .header(
            Row::new(vec!["Property", "Value"]).style(
                Style::default()
                    .fg(th::SUBTEXT1)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .column_spacing(1)
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
                .border_style(Style::default().fg(th::SURFACE1))
                .style(Style::default().fg(th::TEXT).bg(th::BASE))
                .title("Fields"),
        );
    let mut fields_state = TableState::default().with_selected(Some(app.settings_editor_field_idx));
    frame.render_stateful_widget(fields, chunks[2], &mut fields_state);

    let hint = Paragraph::new(if app.settings_text_mode_is_multiline() {
        "j/k field | h/l or <-/-> adjust | Enter newline | Ctrl+S save | Esc cancel | t close"
    } else {
        "j/k field | h/l or <-/-> adjust | Enter edit/save | e edit text | Esc cancel/close | t close"
    })
    .style(Style::default().fg(th::SUBTEXT0));
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
            .border_style(Style::default().fg(th::SURFACE1)),
    )
    .style(Style::default().fg(th::TEXT).bg(th::BASE));
    frame.render_widget(header, chunks[1]);

    let rows = template_editor_rows(app);
    let table_rows = rows
        .iter()
        .enumerate()
        .map(|(idx, (property, value))| {
            let style = if idx == app.template_editor_field_idx {
                Style::default()
                    .fg(th::CRUST)
                    .bg(th::YELLOW)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(th::TEXT)
            };
            Row::new(vec![
                Cell::from(property.clone()),
                Cell::from(value.clone()),
            ])
            .style(style)
        })
        .collect::<Vec<_>>();
    let fields = Table::new(table_rows, [Constraint::Length(34), Constraint::Min(10)])
        .header(
            Row::new(vec!["Property", "Value"]).style(
                Style::default()
                    .fg(th::SUBTEXT1)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .column_spacing(1)
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
                .border_style(Style::default().fg(th::SURFACE1))
                .style(Style::default().fg(th::TEXT).bg(th::BASE))
                .title("Template Fields"),
        );
    let mut fields_state = TableState::default().with_selected(Some(app.template_editor_field_idx));
    frame.render_stateful_widget(fields, chunks[2], &mut fields_state);

    let hint = Paragraph::new(if app.template_editor_text_mode && app.template_editor_is_multiline_field() {
        "j/k field | h/l or <-/-> adjust | Enter newline | Ctrl+S save | Esc cancel | Esc close editor"
    } else {
        "j/k field | h/l or <-/-> adjust | Enter edit/save | e edit text | space toggle | Ctrl+S save template | Esc cancel/close"
    })
    .style(Style::default().fg(th::SUBTEXT0));
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
            String::from("extra-vars (--extra-vars)"),
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
            String::from("extra-vars (--extra-vars)"),
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

fn active_help_text(app: &App) -> String {
    if app.runtime_prompt_open {
        return String::from(
            "Keys: j/k or Up/Down candidate | Enter select runtime | b bootstrap managed runtime | Esc close",
        );
    }
    if app.inventory_create_open {
        return String::from("Keys: Type filename | Enter create | Backspace edit | Esc cancel");
    }
    if app.project_create_open {
        return String::from(
            "Keys: Type text | Up/Down field | Enter next/save | Backspace edit | Esc cancel (mode from n/f/g)",
        );
    }
    if app.project_ssh_open {
        return String::from(
            "Keys: Type text | Up/Down field | Enter next/newline | Ctrl+S save | Backspace edit | Esc cancel",
        );
    }
    if app.inventory_edit_mode_open {
        return String::from(
            "Keys: j/k or Up/Down select mode | Enter confirm | 1/2 quick select | e external | t text | Esc cancel",
        );
    }
    if app.inventory_editor_open {
        return String::from(
            "Keys: Type text | Enter newline | Backspace edit | Ctrl+S save | Esc close",
        );
    }
    if app.settings_editor_open {
        if app.settings_editor_text_mode {
            return if app.settings_text_mode_is_multiline() {
                String::from(
                    "Keys: Type text | Enter newline | Ctrl+S save | Backspace edit | Esc cancel | t close",
                )
            } else {
                String::from("Keys: Type text | Backspace edit | Enter save | Esc cancel | t close")
            };
        }
        return String::from(
            "Keys: j/k field | h/l or <-/-> adjust | Enter or e edit text | space toggle | Esc or t close",
        );
    }
    if app.template_editor_open {
        if app.template_editor_text_mode {
            return if app.template_editor_is_multiline_field() {
                String::from(
                    "Keys: Type text | Enter newline | Ctrl+S save | Backspace edit | Esc cancel",
                )
            } else {
                String::from(
                    "Keys: Type text | Enter save | Ctrl+S save template | Backspace edit | Esc cancel",
                )
            };
        }
        return String::from(
            "Keys: j/k field | h/l or <-/-> adjust | Enter or e edit text | space toggle | Ctrl+S save | Esc close",
        );
    }
    match app.current_view() {
        View::Dashboard => {
            String::from("Keys: Tab/h/l views | r run selected template | u runtime picker | q quit")
        }
        View::Projects => String::from(
            "Keys: j/k or Up/Down select project | Enter/a activate | n new | f import path | g clone git | e ssh key settings | Shift+D delete selected (confirm) | i inventory sync | v vars sync | Tab/h/l views | q quit",
        ),
        View::Inventory => String::from(
            "Keys: j/k or Up/Down select inventory | n new | e edit selected (choose mode) | Shift+D delete selected | PgUp/PgDn scroll logs | End follow latest | Tab/h/l views | q quit",
        ),
        View::Playbooks => {
            if app.log_select_mode {
                String::from(
                    "Keys: j/k or Up/Down select log lines | PgUp/PgDn scroll logs | End follow latest | space mark | y copy | v exit log-select | <-/-> focus playbooks/runs | i/I inventory target | r run | t playbook settings | Tab/h/l views | q quit",
                )
            } else {
                String::from(
                    "Keys: j/k or Up/Down move focused list | PgUp/PgDn scroll logs | End follow latest | <-/-> focus playbooks/runs | Shift+J/K switch runs | i/I inventory target | r run | t playbook settings | v log-select | Tab/h/l views | q quit",
                )
            }
        }
        View::Templates => {
            if app.log_select_mode {
                String::from(
                    "Keys: j/k select log lines | PgUp/PgDn scroll logs | End follow latest | space mark | y copy | v exit log-select | Left/Right focus templates/runs | Shift+J/K switch runs | r run | t/e edit | n new | Shift+D delete | q quit",
                )
            } else {
                String::from(
                    "Keys: j/k move focused list | PgUp/PgDn scroll logs | End follow latest | Left/Right focus templates/runs | Shift+J/K switch runs | r run | t/e edit | n new | Shift+D delete | v log-select | q quit",
                )
            }
        }
        View::Settings => {
            if app.global_settings_text_mode {
                String::from(
                    "Keys: Type text | Backspace edit | Enter save | Esc cancel | j/k field | PgUp/PgDn scroll logs | End follow latest | Tab views | q quit",
                )
            } else {
                String::from(
                    "Keys: j/k field | h/l or <-/-> adjust | Enter or e edit text | space toggle | u runtime picker | PgUp/PgDn scroll logs | End follow latest | Tab views | q quit",
                )
            }
        }
    }
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
            String::from("extra-vars"),
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
            String::from("extra-vars"),
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
            .border_style(Style::default().fg(th::SURFACE1)),
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
                .border_style(Style::default().fg(th::SURFACE1))
                .title(title),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

fn render_environment_breakdown_panel(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    runs_by_environment: &BTreeMap<String, u64>,
    successes_by_environment: &BTreeMap<String, u64>,
) {
    let mut lines = Vec::new();
    if runs_by_environment.is_empty() {
        lines.push(Line::styled(
            "No environment-tagged runs yet.",
            Style::default().fg(th::SUBTEXT0),
        ));
    } else {
        let mut pairs = runs_by_environment
            .iter()
            .map(|(env, count)| (env.clone(), *count))
            .collect::<Vec<_>>();
        pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let max_count = pairs.iter().map(|(_, count)| *count).max().unwrap_or(1);
        let bar_width = area.width.saturating_sub(12) as usize;

        for (env, count) in pairs.into_iter().take(6) {
            let success_count = successes_by_environment.get(&env).copied().unwrap_or(0);
            let success_pct = if count == 0 {
                0
            } else {
                ((success_count as f64 / count as f64) * 100.0).round() as u16
            };
            let fill = if bar_width == 0 {
                0
            } else {
                (((count as f64 / max_count as f64) * bar_width as f64).round() as usize).max(1)
            };
            let color = th::SUBTEXT1;

            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", env.to_uppercase()),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled("█".repeat(fill), Style::default().fg(color)),
                Span::styled(
                    format!(" {count} ({success_pct}%)"),
                    Style::default().fg(th::SUBTEXT1),
                ),
            ]));
        }
    }

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(th::SURFACE1))
                .title("Environment Breakdown"),
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

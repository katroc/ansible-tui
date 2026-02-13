use std::fs;

use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table,
    TableState, Tabs, Wrap,
};
use ratatui::Frame;

use crate::app::{display_path, App, FilterTarget, FocusContext, InventorySubTab};
use crate::theme as th;

use super::common::*;
use super::{HINTS_INVENTORY_CREATE, HINTS_INVENTORY_EDIT_MODE, HINTS_INVENTORY_EDITOR};

pub(super) fn render_inventory(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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

pub(super) fn render_inventory_files(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(area);

    let filtered_inventory_indices = app.filtered_inventory_indices();
    let items = if app.inventories.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::raw("No inventories found. "),
            Span::styled("Add files to ./inventory", Style::default().fg(th::SUBTEXT0)),
        ]))]
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
            .border_style(neutral_border_style())
            .title("File Preview"),
    )
    .wrap(Wrap { trim: false });
    frame.render_widget(preview, right[1]);
}

pub(super) fn render_inventory_hosts(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
    let list_focused = matches!(focus_ctx, FocusContext::InventoryHostsList);
    let detail_focused = matches!(focus_ctx, FocusContext::InventoryHostDetails);
    let list_border = if list_focused {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let host_list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(list_border)
                .style(focus_bg(list_focused))
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
                .border_type(BorderType::Rounded)
                    .border_style(detail_border)
                    .style(focus_bg(detail_focused))
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD))
                .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
                .title("New Variable Name"),
        );
        frame.render_widget(input, prompt_area);
    }
}

pub(super) fn host_field_placeholder(key: &str) -> &'static str {
    match key {
        "ansible_host" => "e.g. 192.0.2.10",
        "ansible_user" => "e.g. ubuntu",
        "ansible_port" => "e.g. 22",
        "ansible_connection" => "e.g. ssh",
        _ => "e.g. value",
    }
}

pub(super) fn render_inventory_groups(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
    let tree_focused = matches!(focus_ctx, FocusContext::InventoryGroupsTree);
    let tree_border = if tree_focused {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let dirty_marker = if state.dirty { " [*]" } else { "" };
    let tree = List::new(tree_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(tree_border)
                .style(focus_bg(tree_focused))
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
    let groups_focused = matches!(focus_ctx, FocusContext::InventoryGroupsGroups);
    let groups_border = if groups_focused {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let groups_list = List::new(group_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(groups_border)
                .style(focus_bg(groups_focused))
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
    let hosts_focused = matches!(focus_ctx, FocusContext::InventoryGroupsHosts);
    let hosts_border = if hosts_focused {
        Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        neutral_border_style()
    };
    let hosts_list = List::new(host_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(hosts_border)
                .style(focus_bg(hosts_focused))
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(th::YELLOW).add_modifier(Modifier::BOLD))
                .style(Style::default().fg(th::TEXT).bg(th::MANTLE))
                .title("New Host Name"),
        );
        frame.render_widget(input, prompt_area);
    }
}

pub(super) fn inventory_detail_rows(app: &App) -> Vec<(String, String)> {
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

pub(super) fn inventory_preview_text(app: &App, max_lines: usize) -> Text<'static> {
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

pub(super) fn render_inventory_create_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(54, 26, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(th::YELLOW))
                .title("Filename"),
        )
        .style(Style::default().fg(th::TEXT).bg(th::BASE));
    frame.render_widget(input, chunks[1]);

    let hint =
        Paragraph::new(hint_line_from_bindings(&HINTS_INVENTORY_CREATE, 4)).style(hint_bar_style());
    frame.render_widget(hint, chunks[2]);
}

pub(super) fn render_inventory_edit_mode_prompt(frame: &mut Frame, app: &App) {
    let area = centered_rect(56, 30, frame.area());
    frame.render_widget(Clear, area);

    let wrapper = Block::default()
        .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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

pub(super) fn render_inventory_editor(frame: &mut Frame, app: &App) {
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .title("Content"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(editor, chunks[1]);

    let hint =
        Paragraph::new(hint_line_from_bindings(&HINTS_INVENTORY_EDITOR, 6)).style(hint_bar_style());
    frame.render_widget(hint, chunks[2]);
}


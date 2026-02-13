use std::collections::{BTreeMap, HashSet};

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    BarChart, Block, BorderType, Borders, Gauge, Paragraph, Sparkline,
    Wrap,
};
use ratatui::Frame;

use crate::app::{display_path, App, RunStatus};
use crate::theme as th;

use super::common::*;

pub(super) fn render_dashboard(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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

pub(super) fn dashboard_stat_card(
    label: &str,
    value: String,
    value_color: ratatui::style::Color,
) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::styled(label.to_string(), Style::default().fg(th::SUBTEXT1)),
        Line::styled(
            value,
            Style::default()
                .fg(value_color)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(neutral_border_style())
            .style(Style::default().bg(th::SURFACE0)),
    )
}

pub(super) fn top_ranked_items(counts: &BTreeMap<String, u64>, max_items: usize) -> (Vec<(String, u64)>, u64) {
    let mut items = counts
        .iter()
        .map(|(name, count)| (name.clone(), *count))
        .collect::<Vec<_>>();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(max_items);

    let max_value = items.iter().map(|(_, count)| *count).max().unwrap_or(1);
    (items, max_value)
}

pub(super) fn last_path_segment(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}

pub(super) fn render_ranked_bar_panel(
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
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .title(title),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

pub(super) fn render_project_summary_panel(
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
                .border_type(BorderType::Rounded)
                .border_style(neutral_border_style())
                .title("Project Summary"),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}


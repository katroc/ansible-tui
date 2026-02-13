use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row};

use crate::app::{App, FocusContext};
use crate::theme as th;

use super::HintBinding;

pub(super) fn neutral_border_style() -> Style {
    Style::default()
        .fg(th::OVERLAY0)
        .add_modifier(Modifier::DIM)
}

/// Returns a subtle `SURFACE0` background tint for focused panels, or
/// transparent for unfocused panels.
pub(super) fn focus_bg(focused: bool) -> Style {
    if focused {
        Style::default().bg(th::SURFACE0)
    } else {
        Style::default()
    }
}

pub(super) fn filtered_list_title(base: &str, query: &str, editing: bool) -> String {
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

pub(super) fn focus_context_label(context: FocusContext) -> &'static str {
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

pub(super) fn status_line_style(app: &App, runtime_required: bool) -> Style {
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

pub(super) fn key_value_value_style(value: &str) -> Style {
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

pub(super) fn styled_key_value_rows(rows: Vec<(String, String)>) -> Vec<Row<'static>> {
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

pub(super) fn key_value_table_layout(
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

pub(super) fn hint_bar_style() -> Style {
    Style::default().fg(th::HINT_TEXT).bg(th::HINT_BG)
}

pub(super) fn hint_line_from_bindings(bindings: &[HintBinding], max_items: usize) -> Line<'static> {
    Line::from(hint_spans_from_bindings(bindings, max_items))
}

pub(super) fn hint_spans_from_bindings(bindings: &[HintBinding], max_items: usize) -> Vec<Span<'static>> {
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

pub(super) fn log_line_style(line: &str) -> Style {
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

pub(super) fn has_nonzero_metric(line: &str, key: &str) -> bool {
    if let Some(pos) = line.find(key) {
        let value = line[pos + key.len()..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>();
        return value.parse::<u64>().map(|v| v > 0).unwrap_or(false);
    }
    false
}

pub(super) fn centered_rect(
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

pub(super) fn masked_secret(value: &str, focused: bool) -> String {
    let stars = "*".repeat(value.chars().count().max(1));
    if focused {
        format!("{stars}|")
    } else {
        stars
    }
}

pub(super) fn summarize_inline_key(value: Option<&str>) -> String {
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


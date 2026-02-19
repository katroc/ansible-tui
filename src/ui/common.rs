use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Row};

use crate::app::{App, FocusContext};
use crate::theme as th;

use super::HintBinding;

pub(super) const HIGHLIGHT_SYMBOL: &str = "▸ ";

pub(super) fn focus_border_style(focused: bool) -> Style {
    if focused {
        th::current().panel_border_focused()
    } else {
        neutral_border_style()
    }
}

pub(super) fn themed_panel(title: impl Into<String>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(focus_border_style(focused))
        .style(focus_bg(focused))
        .title(title.into())
}

pub(super) fn themed_modal(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(th::current().modal_border())
        .style(th::current().modal_bg())
        .title(title.into())
}

pub(super) fn themed_input(title: impl Into<String>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(focus_border_style(focused))
        .title(title.into())
}

pub(super) fn neutral_border_style() -> Style {
    th::current().panel_border()
}

/// Returns themed panel background style.
pub(super) fn focus_bg(focused: bool) -> Style {
    let theme = th::current();
    if focused {
        theme.panel_style_focused()
    } else {
        theme.panel_style()
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
        FocusContext::TaskPreview => "task preview",
        FocusContext::TemplatesList => "templates list",
        FocusContext::TemplatesRuns => "template runs",
        FocusContext::TemplatesLogSelect => "template logs",
        FocusContext::Settings => "settings",
    }
}

pub(super) fn status_line_style(app: &App, runtime_required: bool) -> Style {
    let theme = th::current();
    let base = theme.chrome();
    if app.runtime_prompt_open && !app.runtime_bootstrapping && runtime_required {
        return base.fg(theme.warning).add_modifier(Modifier::BOLD);
    }
    let lowered = app.status_line.to_lowercase();
    if lowered.starts_with("error:")
        || lowered.contains(" failed")
        || lowered.contains("failed:")
        || lowered.contains("failed ")
    {
        return base.fg(theme.error).add_modifier(Modifier::BOLD);
    }
    if lowered.contains("saved")
        || lowered.contains("updated")
        || lowered.contains("ready")
        || lowered.contains("started")
        || lowered.contains("succeeded")
    {
        return base.fg(theme.success);
    }
    if lowered.contains("active project")
        || lowered.contains("runtime")
        || lowered.contains("copied")
        || lowered.contains("selected")
    {
        return theme.status_info().bg(theme.chrome_bg);
    }
    if lowered.contains("loading") || lowered.contains("bootstrap") {
        return base.fg(theme.warning);
    }
    base
}

pub(super) fn key_value_value_style(value: &str) -> Style {
    let theme = th::current();
    let lowered = value.trim().to_ascii_lowercase();
    if lowered == "true" {
        return Style::default().fg(theme.success);
    }
    if lowered == "false" {
        return Style::default().fg(theme.fg_dim);
    }
    if lowered == "unset" || lowered == "none" || lowered == "(none)" {
        return Style::default()
            .fg(theme.fg_dim)
            .add_modifier(Modifier::ITALIC);
    }
    if value.contains('/') || value.contains("\\") {
        return theme.text_link();
    }
    theme.text()
}

pub(super) fn styled_key_value_rows(rows: Vec<(String, String)>) -> Vec<Row<'static>> {
    rows.into_iter()
        .map(|(property, value)| {
            Row::new(vec![
                Cell::from(Line::from(Span::styled(
                    property,
                    th::current().text_muted(),
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
    th::current().hint_bar()
}

pub(super) fn hint_line_from_bindings(bindings: &[HintBinding], max_items: usize) -> Line<'static> {
    Line::from(hint_spans_from_bindings(bindings, max_items))
}

pub(super) fn hint_spans_from_bindings(
    bindings: &[HintBinding],
    max_items: usize,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (idx, hint) in bindings.iter().take(max_items).enumerate() {
        if idx > 0 {
            spans.push(Span::styled("  ·  ", th::current().hint_sep()));
        }
        spans.push(Span::styled(hint.key.to_string(), th::current().hint_key()));
        spans.push(Span::styled(
            format!(" {}", hint.desc),
            th::current().hint_desc(),
        ));
    }
    spans
}

pub(super) fn log_line_style(line: &str) -> Style {
    let theme = th::current();
    if line.starts_with("TASK [") {
        return theme.text_emphasis();
    }
    if line.starts_with("PLAY [") || line.starts_with("PLAY RECAP") {
        return Style::default()
            .fg(theme.warning)
            .add_modifier(Modifier::BOLD);
    }
    if line.starts_with("$ ") {
        return theme.text_muted();
    }
    if line.starts_with("ok:") || (line.contains("failed=0") && line.contains("changed=0")) {
        return Style::default().fg(theme.success);
    }
    if line.starts_with("changed:")
        || (line.contains("changed=") && has_nonzero_metric(line, "changed="))
    {
        return Style::default().fg(theme.warning);
    }
    if line.starts_with("skipping:") {
        return theme.text_dim();
    }
    if line.contains("[stderr]")
        || line.contains("FAILED!")
        || line.starts_with("fatal:")
        || has_nonzero_metric(line, "failed=")
        || has_nonzero_metric(line, "unreachable=")
    {
        return theme.status_error();
    }
    theme.text()
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

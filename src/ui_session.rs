use std::io;
use std::path::Path;

use rusqlite::{params, OptionalExtension};

use crate::db::{open_db, project_db_path, sqlite_to_io};

#[derive(Debug, Clone, Default)]
pub struct UiSessionState {
    pub view_idx: usize,
    pub playbooks_focus_runs: bool,
    pub templates_focus_runs: bool,
    pub log_select_mode: bool,
    pub selected_inventory: Option<String>,
    pub selected_playbook: Option<String>,
    pub selected_template_id: Option<String>,
    pub selected_run_id: Option<u64>,
    pub projects_filter: String,
    pub inventory_files_filter: String,
    pub playbooks_filter: String,
    pub playbook_runs_filter: String,
    pub templates_filter: String,
    pub template_runs_filter: String,
}

pub fn load_ui_session_state(project_root: &Path) -> io::Result<Option<UiSessionState>> {
    let conn = open_ui_session_db(project_root)?;
    conn.query_row(
        "SELECT
             view_idx,
             playbooks_focus_runs,
             templates_focus_runs,
             log_select_mode,
             selected_inventory,
             selected_playbook,
             selected_template_id,
             selected_run_id,
             projects_filter,
             inventory_files_filter,
             playbooks_filter,
             playbook_runs_filter,
             templates_filter,
             template_runs_filter
         FROM ui_session_state
         WHERE id = 1",
        [],
        |row| {
            Ok(UiSessionState {
                view_idx: row.get::<_, i64>(0)?.max(0) as usize,
                playbooks_focus_runs: row.get::<_, i64>(1)? != 0,
                templates_focus_runs: row.get::<_, i64>(2)? != 0,
                log_select_mode: row.get::<_, i64>(3)? != 0,
                selected_inventory: row.get(4)?,
                selected_playbook: row.get(5)?,
                selected_template_id: row.get(6)?,
                selected_run_id: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                projects_filter: row.get(8)?,
                inventory_files_filter: row.get(9)?,
                playbooks_filter: row.get(10)?,
                playbook_runs_filter: row.get(11)?,
                templates_filter: row.get(12)?,
                template_runs_filter: row.get(13)?,
            })
        },
    )
    .optional()
    .map_err(sqlite_to_io)
}

pub fn save_ui_session_state(project_root: &Path, state: &UiSessionState) -> io::Result<()> {
    let conn = open_ui_session_db(project_root)?;
    conn.execute(
        "INSERT INTO ui_session_state (
             id,
             view_idx,
             playbooks_focus_runs,
             templates_focus_runs,
             log_select_mode,
             selected_inventory,
             selected_playbook,
             selected_template_id,
             selected_run_id,
             projects_filter,
             inventory_files_filter,
             playbooks_filter,
             playbook_runs_filter,
             templates_filter,
             template_runs_filter
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
             view_idx = excluded.view_idx,
             playbooks_focus_runs = excluded.playbooks_focus_runs,
             templates_focus_runs = excluded.templates_focus_runs,
             log_select_mode = excluded.log_select_mode,
             selected_inventory = excluded.selected_inventory,
             selected_playbook = excluded.selected_playbook,
             selected_template_id = excluded.selected_template_id,
             selected_run_id = excluded.selected_run_id,
             projects_filter = excluded.projects_filter,
             inventory_files_filter = excluded.inventory_files_filter,
             playbooks_filter = excluded.playbooks_filter,
             playbook_runs_filter = excluded.playbook_runs_filter,
             templates_filter = excluded.templates_filter,
             template_runs_filter = excluded.template_runs_filter",
        params![
            1,
            state.view_idx as i64,
            bool_as_i64(state.playbooks_focus_runs),
            bool_as_i64(state.templates_focus_runs),
            bool_as_i64(state.log_select_mode),
            state.selected_inventory,
            state.selected_playbook,
            state.selected_template_id,
            state.selected_run_id.map(|value| value as i64),
            state.projects_filter,
            state.inventory_files_filter,
            state.playbooks_filter,
            state.playbook_runs_filter,
            state.templates_filter,
            state.template_runs_filter,
        ],
    )
    .map_err(sqlite_to_io)?;
    Ok(())
}

fn open_ui_session_db(project_root: &Path) -> io::Result<rusqlite::Connection> {
    let conn = open_db(&project_db_path(project_root))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ui_session_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            view_idx INTEGER NOT NULL DEFAULT 0,
            playbooks_focus_runs INTEGER NOT NULL DEFAULT 0,
            templates_focus_runs INTEGER NOT NULL DEFAULT 0,
            log_select_mode INTEGER NOT NULL DEFAULT 0,
            selected_inventory TEXT,
            selected_playbook TEXT,
            selected_template_id TEXT,
            selected_run_id INTEGER,
            projects_filter TEXT NOT NULL DEFAULT '',
            inventory_files_filter TEXT NOT NULL DEFAULT '',
            playbooks_filter TEXT NOT NULL DEFAULT '',
            playbook_runs_filter TEXT NOT NULL DEFAULT '',
            templates_filter TEXT NOT NULL DEFAULT '',
            template_runs_filter TEXT NOT NULL DEFAULT ''
        )",
    )
    .map_err(sqlite_to_io)?;
    Ok(conn)
}

fn bool_as_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_project_root(name: &str) -> std::path::PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ansible_tui_ui_session_{name}_{suffix}"))
    }

    #[test]
    fn load_returns_none_when_no_state_exists() {
        let root = temp_project_root("empty");
        let loaded = load_ui_session_state(&root).expect("load state");
        assert!(loaded.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn save_and_load_round_trip() {
        let root = temp_project_root("round_trip");
        let state = UiSessionState {
            view_idx: 4,
            playbooks_focus_runs: true,
            templates_focus_runs: false,
            log_select_mode: true,
            selected_inventory: Some(String::from("inventories/prod.yml")),
            selected_playbook: Some(String::from("playbooks/site.yml")),
            selected_template_id: Some(String::from("deploy-123456")),
            selected_run_id: Some(42),
            projects_filter: String::from("prod"),
            inventory_files_filter: String::from("k8s"),
            playbooks_filter: String::from("deploy"),
            playbook_runs_filter: String::from("failed"),
            templates_filter: String::from("nightly"),
            template_runs_filter: String::from("#042"),
        };
        save_ui_session_state(&root, &state).expect("save state");
        let loaded = load_ui_session_state(&root)
            .expect("load state")
            .expect("saved state should exist");
        assert_eq!(loaded.view_idx, state.view_idx);
        assert_eq!(loaded.playbooks_focus_runs, state.playbooks_focus_runs);
        assert_eq!(loaded.templates_focus_runs, state.templates_focus_runs);
        assert_eq!(loaded.log_select_mode, state.log_select_mode);
        assert_eq!(loaded.selected_inventory, state.selected_inventory);
        assert_eq!(loaded.selected_playbook, state.selected_playbook);
        assert_eq!(loaded.selected_template_id, state.selected_template_id);
        assert_eq!(loaded.selected_run_id, state.selected_run_id);
        assert_eq!(loaded.projects_filter, state.projects_filter);
        assert_eq!(loaded.inventory_files_filter, state.inventory_files_filter);
        assert_eq!(loaded.playbooks_filter, state.playbooks_filter);
        assert_eq!(loaded.playbook_runs_filter, state.playbook_runs_filter);
        assert_eq!(loaded.templates_filter, state.templates_filter);
        assert_eq!(loaded.template_runs_filter, state.template_runs_filter);
        let _ = fs::remove_dir_all(root);
    }
}

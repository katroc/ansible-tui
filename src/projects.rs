use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::params;

use crate::db::{global_db_path, open_db, sqlite_to_io};
use crate::secrets::VaultSourceType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDefinition {
    pub name: String,
    pub root: PathBuf,
    pub inventory_sync_cmd: Option<String>,
    pub vars_sync_cmd: Option<String>,
    pub ssh_private_key_file: Option<String>,
    pub ssh_private_key_inline: Option<String>,
    pub vault_source_type: Option<VaultSourceType>,
    pub vault_password_file: Option<String>,
    pub vault_id_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectRegistry {
    pub projects: Vec<ProjectDefinition>,
    pub active_idx: usize,
}

const CONFIG_DIR: &str = ".ansible-tui";
const LEGACY_PROJECTS_FILE: &str = "projects.tsv";

pub fn load_projects(cwd: &Path) -> io::Result<ProjectRegistry> {
    let conn = open_projects_db(cwd)?;
    migrate_from_tsv_if_needed(cwd, &conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT idx, name, root, inventory_sync_cmd, vars_sync_cmd, \
             ssh_private_key_file, ssh_private_key_inline, \
             vault_source_type, vault_password_file, vault_id_label, is_active \
             FROM projects ORDER BY idx ASC",
        )
        .map_err(sqlite_to_io)?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })
        .map_err(sqlite_to_io)?;

    let mut projects = Vec::new();
    let mut active_idx = 0usize;
    let mut saw_active = false;

    for row in rows {
        let (
            _idx,
            name,
            root_str,
            inv_sync,
            vars_sync,
            ssh_key_file,
            ssh_key_inline,
            vault_type,
            vault_pass,
            vault_label,
            is_active,
        ) = row.map_err(sqlite_to_io)?;

        let root = resolve_root(cwd, &root_str);
        let inferred_name = if name.is_empty() {
            root.file_name()
                .and_then(|v| v.to_str())
                .filter(|v| !v.trim().is_empty())
                .unwrap_or("Project")
                .to_string()
        } else {
            name
        };

        projects.push(ProjectDefinition {
            name: inferred_name,
            root,
            inventory_sync_cmd: inv_sync,
            vars_sync_cmd: vars_sync,
            ssh_private_key_file: ssh_key_file,
            ssh_private_key_inline: ssh_key_inline,
            vault_source_type: vault_type.as_deref().and_then(VaultSourceType::from_str),
            vault_password_file: vault_pass,
            vault_id_label: vault_label,
        });

        if is_active != 0 && !saw_active {
            active_idx = projects.len().saturating_sub(1);
            saw_active = true;
        }
    }

    if projects.is_empty() {
        projects.push(default_project(cwd));
        active_idx = 0;
    } else if active_idx >= projects.len() {
        active_idx = 0;
    }

    Ok(ProjectRegistry {
        projects,
        active_idx,
    })
}

pub fn save_projects(cwd: &Path, registry: &ProjectRegistry) -> io::Result<()> {
    let conn = open_projects_db(cwd)?;
    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    tx.execute("DELETE FROM projects", [])
        .map_err(sqlite_to_io)?;

    let mut insert = tx
        .prepare(
            "INSERT INTO projects (idx, name, root, inventory_sync_cmd, vars_sync_cmd, \
             ssh_private_key_file, ssh_private_key_inline, \
             vault_source_type, vault_password_file, vault_id_label, is_active) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .map_err(sqlite_to_io)?;

    for (idx, project) in registry.projects.iter().enumerate() {
        let root = store_root(cwd, &project.root);
        let is_active: i64 = if idx == registry.active_idx { 1 } else { 0 };
        insert
            .execute(params![
                idx as i64,
                project.name,
                root,
                project.inventory_sync_cmd,
                project.vars_sync_cmd,
                project.ssh_private_key_file,
                project.ssh_private_key_inline,
                project.vault_source_type.map(|v| v.as_str().to_string()),
                project.vault_password_file,
                project.vault_id_label,
                is_active,
            ])
            .map_err(sqlite_to_io)?;
    }

    drop(insert);
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

pub fn default_project(cwd: &Path) -> ProjectDefinition {
    ProjectDefinition {
        name: String::from("Local"),
        root: cwd.to_path_buf(),
        inventory_sync_cmd: None,
        vars_sync_cmd: None,
        ssh_private_key_file: None,
        ssh_private_key_inline: None,
        vault_source_type: None,
        vault_password_file: None,
        vault_id_label: None,
    }
}

// --- private helpers ---

fn open_projects_db(cwd: &Path) -> io::Result<rusqlite::Connection> {
    let conn = open_db(&global_db_path(cwd))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS projects (
            idx INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            root TEXT NOT NULL,
            inventory_sync_cmd TEXT,
            vars_sync_cmd TEXT,
            ssh_private_key_file TEXT,
            ssh_private_key_inline TEXT,
            vault_source_type TEXT,
            vault_password_file TEXT,
            vault_id_label TEXT,
            is_active INTEGER NOT NULL DEFAULT 0
         )",
    )
    .map_err(sqlite_to_io)?;
    Ok(conn)
}

fn migrate_from_tsv_if_needed(cwd: &Path, conn: &rusqlite::Connection) -> io::Result<()> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
        .map_err(sqlite_to_io)?;
    if count > 0 {
        return Ok(());
    }

    let tsv_path = cwd.join(CONFIG_DIR).join(LEGACY_PROJECTS_FILE);
    let data = match fs::read_to_string(tsv_path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    let mut projects = Vec::new();
    let mut active_idx = 0usize;
    let mut saw_active = false;

    for line in data.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() < 2 {
            continue;
        }

        let name = fields[0].trim().to_string();
        let root_str = fields[1].trim().to_string();
        let inventory_sync_cmd = fields.get(2).and_then(|v| to_opt_text(v));
        let vars_sync_cmd = fields.get(3).and_then(|v| to_opt_text(v));
        let ssh_private_key_file = if fields.len() >= 7 {
            fields.get(4).and_then(|v| to_opt_text(v))
        } else {
            None
        };
        let ssh_private_key_inline = if fields.len() >= 7 {
            fields.get(5).and_then(|v| to_opt_multiline_text(v))
        } else {
            None
        };
        let (vault_source_type, vault_password_file, vault_id_label) = if fields.len() >= 10 {
            (
                fields
                    .get(6)
                    .and_then(|v| VaultSourceType::from_str(v.trim())),
                fields.get(7).and_then(|v| to_opt_text(v)),
                fields.get(8).and_then(|v| to_opt_text(v)),
            )
        } else {
            (None, None, None)
        };
        let active_field_idx = if fields.len() >= 10 {
            9
        } else if fields.len() >= 7 {
            6
        } else {
            4
        };
        let is_active = fields
            .get(active_field_idx)
            .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
            .unwrap_or(false);

        projects.push((
            name,
            root_str,
            inventory_sync_cmd,
            vars_sync_cmd,
            ssh_private_key_file,
            ssh_private_key_inline,
            vault_source_type.map(|v| v.as_str().to_string()),
            vault_password_file,
            vault_id_label,
            is_active,
        ));

        if is_active && !saw_active {
            active_idx = projects.len().saturating_sub(1);
            saw_active = true;
        }
    }

    if projects.is_empty() {
        return Ok(());
    }

    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    let mut insert = tx
        .prepare(
            "INSERT OR IGNORE INTO projects (idx, name, root, inventory_sync_cmd, vars_sync_cmd, \
             ssh_private_key_file, ssh_private_key_inline, \
             vault_source_type, vault_password_file, vault_id_label, is_active) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .map_err(sqlite_to_io)?;

    for (idx, proj) in projects.iter().enumerate() {
        let is_active_val: i64 = if idx == active_idx { 1 } else { 0 };
        insert
            .execute(params![
                idx as i64,
                proj.0,
                proj.1,
                proj.2,
                proj.3,
                proj.4,
                proj.5,
                proj.6,
                proj.7,
                proj.8,
                is_active_val,
            ])
            .map_err(sqlite_to_io)?;
    }

    drop(insert);
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

fn to_opt_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn to_opt_multiline_text(value: &str) -> Option<String> {
    let unescaped = unescape_multiline_field(value);
    if unescaped.trim().is_empty() {
        None
    } else {
        Some(unescaped)
    }
}

fn unescape_multiline_field(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn resolve_root(cwd: &Path, value: &str) -> PathBuf {
    let raw = value.trim();
    if raw.is_empty() {
        return cwd.to_path_buf();
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else if raw == "." {
        cwd.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn store_root(cwd: &Path, root: &Path) -> String {
    root.strip_prefix(cwd)
        .map(|p| {
            let s = p.display().to_string();
            if s.is_empty() {
                String::from(".")
            } else {
                s
            }
        })
        .unwrap_or_else(|_| root.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cwd(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ansible_tui_projects_{name}_{suffix}"))
    }

    #[test]
    fn save_and_load_round_trip() {
        let cwd = temp_cwd("round_trip_db");
        let registry = ProjectRegistry {
            projects: vec![ProjectDefinition {
                name: String::from("Ops"),
                root: cwd.clone(),
                inventory_sync_cmd: Some(String::from("./sync-inventory.sh")),
                vars_sync_cmd: Some(String::from("./sync-vars.sh")),
                ssh_private_key_file: Some(String::from(".keys/ops.pem")),
                ssh_private_key_inline: None,
                vault_source_type: Some(VaultSourceType::File),
                vault_password_file: Some(String::from(".secrets/vault-pass.txt")),
                vault_id_label: Some(String::from("ops")),
            }],
            active_idx: 0,
        };

        save_projects(&cwd, &registry).expect("save projects");
        let loaded = load_projects(&cwd).expect("load projects");
        assert_eq!(loaded.projects.len(), 1);
        let project = &loaded.projects[0];
        assert_eq!(project.name, "Ops");
        assert_eq!(project.root, cwd);
        assert_eq!(project.vault_source_type, Some(VaultSourceType::File));
        assert_eq!(
            project.vault_password_file.as_deref(),
            Some(".secrets/vault-pass.txt")
        );
        assert_eq!(project.vault_id_label.as_deref(), Some("ops"));

        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn migrate_from_legacy_tsv() {
        let cwd = temp_cwd("migrate_tsv");
        let settings_dir = cwd.join(".ansible-tui");
        fs::create_dir_all(&settings_dir).expect("create settings dir");
        let content = "Local\t.\t\t\t\t\t1\n";
        fs::write(settings_dir.join("projects.tsv"), content).expect("write projects.tsv");

        let registry = load_projects(&cwd).expect("load projects");
        assert_eq!(registry.projects.len(), 1);
        let project = &registry.projects[0];
        assert_eq!(project.name, "Local");
        assert_eq!(project.root, cwd);
        assert_eq!(project.vault_source_type, None);

        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn empty_dir_returns_default_project() {
        let cwd = temp_cwd("empty");
        let registry = load_projects(&cwd).expect("load projects");
        assert_eq!(registry.projects.len(), 1);
        assert_eq!(registry.projects[0].name, "Local");
        let _ = fs::remove_dir_all(&cwd);
    }
}

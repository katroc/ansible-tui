use std::fs;
use std::io;
use std::path::Path;

use rusqlite::params;

use crate::db::{open_db, project_db_path, sqlite_to_io};
use crate::run::RunOptions;
use crate::secrets::VaultSourceType;

#[derive(Debug, Clone)]
pub struct JobTemplate {
    pub id: String,
    pub name: String,
    pub playbook: String,
    pub inventory: String,
    pub check: bool,
    pub diff: bool,
    pub become_enabled: bool,
    pub verbosity: u8,
    pub forks: Option<u16>,
    pub timeout: Option<u16>,
    pub limit: Option<String>,
    pub tags: Option<String>,
    pub extra_vars: Option<String>,
    pub extra_args: Option<String>,
    pub ssh_private_key_file: Option<String>,
    pub ssh_private_key_inline: Option<String>,
    pub vault_source_type: Option<VaultSourceType>,
    pub vault_password_file: Option<String>,
    pub vault_id_label: Option<String>,
}

impl JobTemplate {
    pub fn new(name: &str) -> Self {
        Self {
            id: generate_id(name),
            name: name.to_string(),
            playbook: String::new(),
            inventory: String::new(),
            check: false,
            diff: false,
            become_enabled: false,
            verbosity: 0,
            forks: None,
            timeout: None,
            limit: None,
            tags: None,
            extra_vars: None,
            extra_args: None,
            ssh_private_key_file: None,
            ssh_private_key_inline: None,
            vault_source_type: None,
            vault_password_file: None,
            vault_id_label: None,
        }
    }

    pub fn to_run_options(&self, ansible_bin: &str) -> RunOptions {
        RunOptions {
            ansible_bin: ansible_bin.to_string(),
            check: self.check,
            diff: self.diff,
            become_enabled: self.become_enabled,
            verbosity: self.verbosity,
            forks: self.forks,
            timeout: self.timeout,
            limit: self.limit.clone(),
            tags: self.tags.clone(),
            extra_vars_files: Vec::new(),
            extra_vars: self.extra_vars.clone(),
            extra_args: self.extra_args.clone(),
            ssh_private_key_file: self.ssh_private_key_file.clone(),
            ssh_private_key_inline: self.ssh_private_key_inline.clone(),
            vault_source_type: self.vault_source_type,
            vault_password_file: self.vault_password_file.clone(),
            vault_id_label: self.vault_id_label.clone(),
        }
    }
}

fn generate_id(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    let slug = if slug.is_empty() {
        String::from("template")
    } else {
        slug
    };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        % 1_000_000;
    format!("{}-{:06}", slug, ts)
}

const SETTINGS_DIR: &str = ".ansible-tui";
const LEGACY_TEMPLATES_FILE: &str = "job_templates.tsv";

pub fn load_job_templates(cwd: &Path) -> io::Result<Vec<JobTemplate>> {
    let conn = open_templates_db(cwd)?;
    migrate_from_tsv_if_needed(cwd, &conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT id, name, playbook, inventory, \
             \"check\", diff, become_enabled, verbosity, \
             forks, timeout, \"limit\", tags, extra_vars, extra_args, \
             ssh_private_key_file, ssh_private_key_inline, \
             vault_source_type, vault_password_file, vault_id_label \
             FROM job_templates ORDER BY rowid ASC",
        )
        .map_err(sqlite_to_io)?;

    let rows = stmt
        .query_map([], |row| {
            Ok(JobTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                playbook: row.get(2)?,
                inventory: row.get(3)?,
                check: row.get::<_, i64>(4)? != 0,
                diff: row.get::<_, i64>(5)? != 0,
                become_enabled: row.get::<_, i64>(6)? != 0,
                verbosity: row.get::<_, i64>(7)?.min(4) as u8,
                forks: row.get::<_, Option<i64>>(8)?.map(|v| v as u16),
                timeout: row.get::<_, Option<i64>>(9)?.map(|v| v as u16),
                limit: row.get(10)?,
                tags: row.get(11)?,
                extra_vars: row.get(12)?,
                extra_args: row.get(13)?,
                ssh_private_key_file: row.get(14)?,
                ssh_private_key_inline: row.get(15)?,
                vault_source_type: row
                    .get::<_, Option<String>>(16)?
                    .as_deref()
                    .and_then(VaultSourceType::from_str),
                vault_password_file: row.get(17)?,
                vault_id_label: row.get(18)?,
            })
        })
        .map_err(sqlite_to_io)?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sqlite_to_io)?);
    }
    Ok(out)
}

pub fn save_job_templates(cwd: &Path, templates: &[JobTemplate]) -> io::Result<()> {
    let conn = open_templates_db(cwd)?;
    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    tx.execute("DELETE FROM job_templates", [])
        .map_err(sqlite_to_io)?;

    let mut insert = tx
        .prepare(
            "INSERT INTO job_templates \
             (id, name, playbook, inventory, \
              \"check\", diff, become_enabled, verbosity, \
              forks, timeout, \"limit\", tags, extra_vars, extra_args, \
              ssh_private_key_file, ssh_private_key_inline, \
              vault_source_type, vault_password_file, vault_id_label) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
        )
        .map_err(sqlite_to_io)?;

    for t in templates {
        insert
            .execute(params![
                t.id,
                t.name,
                t.playbook,
                t.inventory,
                t.check as i64,
                t.diff as i64,
                t.become_enabled as i64,
                t.verbosity as i64,
                t.forks.map(|v| v as i64),
                t.timeout.map(|v| v as i64),
                t.limit,
                t.tags,
                t.extra_vars,
                t.extra_args,
                t.ssh_private_key_file,
                t.ssh_private_key_inline,
                t.vault_source_type.map(|v| v.as_str().to_string()),
                t.vault_password_file,
                t.vault_id_label,
            ])
            .map_err(sqlite_to_io)?;
    }

    drop(insert);
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

// --- private helpers ---

fn open_templates_db(cwd: &Path) -> io::Result<rusqlite::Connection> {
    let conn = open_db(&project_db_path(cwd))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS job_templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            playbook TEXT NOT NULL,
            inventory TEXT NOT NULL,
            \"check\" INTEGER NOT NULL DEFAULT 0,
            diff INTEGER NOT NULL DEFAULT 0,
            become_enabled INTEGER NOT NULL DEFAULT 0,
            verbosity INTEGER NOT NULL DEFAULT 0,
            forks INTEGER,
            timeout INTEGER,
            \"limit\" TEXT,
            tags TEXT,
            extra_vars TEXT,
            extra_args TEXT,
            ssh_private_key_file TEXT,
            ssh_private_key_inline TEXT,
            vault_source_type TEXT,
            vault_password_file TEXT,
            vault_id_label TEXT
         )",
    )
    .map_err(sqlite_to_io)?;
    Ok(conn)
}

fn migrate_from_tsv_if_needed(cwd: &Path, conn: &rusqlite::Connection) -> io::Result<()> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM job_templates", [], |row| row.get(0))
        .map_err(sqlite_to_io)?;
    if count > 0 {
        return Ok(());
    }

    let tsv_path = cwd.join(SETTINGS_DIR).join(LEGACY_TEMPLATES_FILE);
    let data = match fs::read_to_string(tsv_path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    let mut insert = tx
        .prepare(
            "INSERT OR IGNORE INTO job_templates \
             (id, name, playbook, inventory, \
              \"check\", diff, become_enabled, verbosity, \
              forks, timeout, \"limit\", tags, extra_vars, extra_args, \
              ssh_private_key_file, ssh_private_key_inline, \
              vault_source_type, vault_password_file, vault_id_label) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
        )
        .map_err(sqlite_to_io)?;

    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(t) = parse_tsv_line(line) {
            insert
                .execute(params![
                    t.id,
                    t.name,
                    t.playbook,
                    t.inventory,
                    t.check as i64,
                    t.diff as i64,
                    t.become_enabled as i64,
                    t.verbosity as i64,
                    t.forks.map(|v| v as i64),
                    t.timeout.map(|v| v as i64),
                    t.limit,
                    t.tags,
                    t.extra_vars,
                    t.extra_args,
                    t.ssh_private_key_file,
                    t.ssh_private_key_inline,
                    t.vault_source_type.map(|v| v.as_str().to_string()),
                    t.vault_password_file,
                    t.vault_id_label,
                ])
                .map_err(sqlite_to_io)?;
        }
    }

    drop(insert);
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

// --- TSV legacy parsing ---

fn parse_tsv_line(line: &str) -> Option<JobTemplate> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() < 16 {
        return None;
    }

    let mut idx = 0usize;
    let id = tsv_unescape(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    if id.is_empty() {
        return None;
    }
    let name = tsv_unescape(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let playbook = tsv_unescape(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let inventory = tsv_unescape(fields.get(idx).copied().unwrap_or_default());
    idx += 1;

    // Backward compatibility: legacy rows included an `environment` field after inventory.
    if !is_bool_like(fields.get(idx).copied().unwrap_or_default()) {
        idx += 1;
    }
    if fields.len() < idx + 12 {
        return None;
    }

    let check = parse_bool(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let diff = parse_bool(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let become_enabled = parse_bool(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let verbosity = fields
        .get(idx)
        .copied()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0)
        .min(4);
    idx += 1;
    let forks = parse_opt_u16(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let timeout = parse_opt_u16(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let limit = parse_opt_string(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let tags = parse_opt_string(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let extra_vars = parse_opt_string(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let extra_args = parse_opt_string(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let ssh_private_key_file = parse_opt_string(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let ssh_private_key_inline = parse_opt_string(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let vault_source_type = fields
        .get(idx)
        .and_then(|raw| VaultSourceType::from_str(raw.trim()));
    idx += 1;
    let vault_password_file = parse_opt_string(fields.get(idx).copied().unwrap_or_default());
    idx += 1;
    let vault_id_label = parse_opt_string(fields.get(idx).copied().unwrap_or_default());

    Some(JobTemplate {
        id,
        name,
        playbook,
        inventory,
        check,
        diff,
        become_enabled,
        verbosity,
        forks,
        timeout,
        limit,
        tags,
        extra_vars,
        extra_args,
        ssh_private_key_file,
        ssh_private_key_inline,
        vault_source_type,
        vault_password_file,
        vault_id_label,
    })
}

fn is_bool_like(raw: &str) -> bool {
    matches!(raw.trim(), "1" | "0" | "true" | "false" | "yes" | "no")
}

fn parse_bool(raw: &str) -> bool {
    matches!(raw.trim(), "1" | "true" | "yes")
}

fn parse_opt_u16(raw: &str) -> Option<u16> {
    let raw = raw.trim();
    if raw.is_empty() {
        None
    } else {
        raw.parse::<u16>().ok()
    }
}

fn parse_opt_string(raw: &str) -> Option<String> {
    let raw = tsv_unescape(raw);
    let raw = raw.trim().to_string();
    if raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

fn tsv_unescape(raw: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_cwd(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ansible_tui_templates_{name}_{suffix}"))
    }

    #[test]
    fn save_and_load_round_trip() {
        let cwd = temp_cwd("round_trip");
        let templates = vec![
            JobTemplate {
                id: String::from("deploy-web-123456"),
                name: String::from("Deploy Web"),
                playbook: String::from("playbooks/deploy.yml"),
                inventory: String::from("inventory/prod.yml"),
                check: false,
                diff: true,
                become_enabled: true,
                verbosity: 2,
                forks: Some(10),
                timeout: Some(60),
                limit: Some(String::from("webservers")),
                tags: Some(String::from("deploy,restart")),
                extra_vars: Some(String::from("{\"version\": \"1.0\"}")),
                extra_args: Some(String::from("--force-handlers")),
                ssh_private_key_file: Some(String::from("~/.ssh/deploy_key")),
                ssh_private_key_inline: None,
                vault_source_type: Some(VaultSourceType::Prompt),
                vault_password_file: None,
                vault_id_label: Some(String::from("prod")),
            },
            JobTemplate {
                id: String::from("simple-000001"),
                name: String::from("Simple"),
                playbook: String::from("site.yml"),
                inventory: String::from("hosts"),
                check: false,
                diff: false,
                become_enabled: false,
                verbosity: 0,
                forks: None,
                timeout: None,
                limit: None,
                tags: None,
                extra_vars: None,
                extra_args: None,
                ssh_private_key_file: None,
                ssh_private_key_inline: None,
                vault_source_type: None,
                vault_password_file: Some(String::from("~/.vault-pass")),
                vault_id_label: None,
            },
        ];

        save_job_templates(&cwd, &templates).expect("save");
        let loaded = load_job_templates(&cwd).expect("load");
        assert_eq!(loaded.len(), 2);

        for (original, restored) in templates.iter().zip(loaded.iter()) {
            assert_eq!(original.id, restored.id);
            assert_eq!(original.name, restored.name);
            assert_eq!(original.playbook, restored.playbook);
            assert_eq!(original.inventory, restored.inventory);
            assert_eq!(original.check, restored.check);
            assert_eq!(original.diff, restored.diff);
            assert_eq!(original.become_enabled, restored.become_enabled);
            assert_eq!(original.verbosity, restored.verbosity);
            assert_eq!(original.forks, restored.forks);
            assert_eq!(original.timeout, restored.timeout);
            assert_eq!(original.limit, restored.limit);
            assert_eq!(original.tags, restored.tags);
            assert_eq!(original.extra_vars, restored.extra_vars);
            assert_eq!(original.extra_args, restored.extra_args);
            assert_eq!(original.ssh_private_key_file, restored.ssh_private_key_file);
            assert_eq!(
                original.ssh_private_key_inline,
                restored.ssh_private_key_inline
            );
            assert_eq!(original.vault_source_type, restored.vault_source_type);
            assert_eq!(original.vault_password_file, restored.vault_password_file);
            assert_eq!(original.vault_id_label, restored.vault_id_label);
        }

        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn test_generate_id() {
        let id = generate_id("Deploy Web App");
        assert!(id.starts_with("deploy-web-app-"));
        assert!(id.len() > 15);

        let id = generate_id("");
        assert!(id.starts_with("template-"));
    }
}

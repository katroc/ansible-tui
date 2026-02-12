use std::fs;
use std::io;
use std::path::Path;

use rusqlite::params;

use crate::db::{global_db_path, open_db, sqlite_to_io};
use crate::secrets::SecretEnforcementMode;

#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    pub ansible_bin: Option<String>,
    pub check: Option<bool>,
    pub diff: Option<bool>,
    pub become_enabled: Option<bool>,
    pub verbosity: Option<u8>,
    pub forks: Option<u16>,
    pub timeout: Option<u16>,
    pub limit: Option<String>,
    pub tags: Option<String>,
    pub extra_vars: Option<String>,
    pub extra_args: Option<String>,
    pub secret_enforcement_mode: Option<SecretEnforcementMode>,
}

const CONFIG_DIR: &str = ".ansible-tui";
const LEGACY_CONFIG_FILE: &str = "config.env";

pub fn load_app_config(cwd: &Path) -> io::Result<AppConfig> {
    let conn = open_global_db(cwd)?;
    migrate_from_env_if_needed(cwd, &conn)?;

    let mut stmt = conn
        .prepare("SELECT key, value FROM app_config")
        .map_err(sqlite_to_io)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sqlite_to_io)?;

    let mut config = AppConfig::default();
    for row in rows {
        let (key, value) = row.map_err(sqlite_to_io)?;
        match key.as_str() {
            "ansible_bin" => config.ansible_bin = parse_opt_string(&value),
            "check" => config.check = parse_opt_bool(&value),
            "diff" => config.diff = parse_opt_bool(&value),
            "become_enabled" => config.become_enabled = parse_opt_bool(&value),
            "verbosity" => config.verbosity = parse_opt_u8(&value),
            "forks" => config.forks = parse_opt_u16(&value),
            "timeout" => config.timeout = parse_opt_u16(&value),
            "limit" => config.limit = parse_opt_string(&value),
            "tags" => config.tags = parse_opt_string(&value),
            "extra_vars" => config.extra_vars = parse_opt_string(&value),
            "extra_args" => config.extra_args = parse_opt_string(&value),
            "secret_enforcement_mode" => {
                config.secret_enforcement_mode = SecretEnforcementMode::from_str(&value)
            }
            _ => {}
        }
    }

    Ok(config)
}

pub fn save_app_config(cwd: &Path, config: &AppConfig) -> io::Result<()> {
    let conn = open_global_db(cwd)?;
    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    tx.execute("DELETE FROM app_config", [])
        .map_err(sqlite_to_io)?;

    let mut insert = tx
        .prepare("INSERT INTO app_config (key, value) VALUES (?1, ?2)")
        .map_err(sqlite_to_io)?;

    if let Some(ref v) = config.ansible_bin {
        insert
            .execute(params!["ansible_bin", v])
            .map_err(sqlite_to_io)?;
    }
    if let Some(v) = config.check {
        insert
            .execute(params!["check", bool_str(v)])
            .map_err(sqlite_to_io)?;
    }
    if let Some(v) = config.diff {
        insert
            .execute(params!["diff", bool_str(v)])
            .map_err(sqlite_to_io)?;
    }
    if let Some(v) = config.become_enabled {
        insert
            .execute(params!["become_enabled", bool_str(v)])
            .map_err(sqlite_to_io)?;
    }
    if let Some(v) = config.verbosity {
        insert
            .execute(params!["verbosity", v.to_string()])
            .map_err(sqlite_to_io)?;
    }
    if let Some(v) = config.forks {
        insert
            .execute(params!["forks", v.to_string()])
            .map_err(sqlite_to_io)?;
    }
    if let Some(v) = config.timeout {
        insert
            .execute(params!["timeout", v.to_string()])
            .map_err(sqlite_to_io)?;
    }
    if let Some(ref v) = config.limit {
        insert.execute(params!["limit", v]).map_err(sqlite_to_io)?;
    }
    if let Some(ref v) = config.tags {
        insert.execute(params!["tags", v]).map_err(sqlite_to_io)?;
    }
    if let Some(ref v) = config.extra_vars {
        insert
            .execute(params!["extra_vars", v])
            .map_err(sqlite_to_io)?;
    }
    if let Some(ref v) = config.extra_args {
        insert
            .execute(params!["extra_args", v])
            .map_err(sqlite_to_io)?;
    }
    if let Some(v) = config.secret_enforcement_mode {
        insert
            .execute(params!["secret_enforcement_mode", v.as_str()])
            .map_err(sqlite_to_io)?;
    }

    drop(insert);
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

fn open_global_db(cwd: &Path) -> io::Result<rusqlite::Connection> {
    let conn = open_db(&global_db_path(cwd))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_config (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    )
    .map_err(sqlite_to_io)?;
    Ok(conn)
}

fn migrate_from_env_if_needed(cwd: &Path, conn: &rusqlite::Connection) -> io::Result<()> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM app_config", [], |row| row.get(0))
        .map_err(sqlite_to_io)?;
    if count > 0 {
        return Ok(());
    }

    let legacy_path = cwd.join(CONFIG_DIR).join(LEGACY_CONFIG_FILE);
    let data = match fs::read_to_string(legacy_path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    let mut insert = tx
        .prepare("INSERT OR IGNORE INTO app_config (key, value) VALUES (?1, ?2)")
        .map_err(sqlite_to_io)?;

    for line in data.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if !value.is_empty() {
                insert.execute(params![key, value]).map_err(sqlite_to_io)?;
            }
        }
    }

    drop(insert);
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

fn bool_str(v: bool) -> &'static str {
    if v {
        "1"
    } else {
        "0"
    }
}

fn parse_opt_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_opt_bool(value: &str) -> Option<bool> {
    match value {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

fn parse_opt_u8(value: &str) -> Option<u8> {
    value.parse::<u8>().ok()
}

fn parse_opt_u16(value: &str) -> Option<u16> {
    value.parse::<u16>().ok()
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
        std::env::temp_dir().join(format!("ansible_tui_config_{name}_{suffix}"))
    }

    #[test]
    fn save_and_load_round_trip() {
        let cwd = temp_cwd("round_trip");
        let config = AppConfig {
            ansible_bin: Some(String::from("/usr/bin/ansible-playbook")),
            check: Some(true),
            diff: Some(false),
            become_enabled: None,
            verbosity: Some(2),
            forks: Some(10),
            timeout: None,
            limit: Some(String::from("web")),
            tags: None,
            extra_vars: Some(String::from("foo=bar")),
            extra_args: None,
            secret_enforcement_mode: Some(SecretEnforcementMode::Strict),
        };
        save_app_config(&cwd, &config).expect("save");
        let loaded = load_app_config(&cwd).expect("load");
        assert_eq!(loaded.ansible_bin, config.ansible_bin);
        assert_eq!(loaded.check, config.check);
        assert_eq!(loaded.diff, config.diff);
        assert_eq!(loaded.become_enabled, None);
        assert_eq!(loaded.verbosity, config.verbosity);
        assert_eq!(loaded.forks, config.forks);
        assert_eq!(loaded.timeout, None);
        assert_eq!(loaded.limit, config.limit);
        assert_eq!(loaded.extra_vars, config.extra_vars);
        assert_eq!(
            loaded.secret_enforcement_mode,
            Some(SecretEnforcementMode::Strict)
        );
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn migrate_from_legacy_env_file() {
        let cwd = temp_cwd("migrate_env");
        fs::create_dir_all(cwd.join(".ansible-tui")).expect("create dir");
        fs::write(
            cwd.join(".ansible-tui").join("config.env"),
            "secret_enforcement_mode=compat\nverbosity=3\n",
        )
        .expect("write legacy");

        let config = load_app_config(&cwd).expect("load");
        assert_eq!(
            config.secret_enforcement_mode,
            Some(SecretEnforcementMode::Compat)
        );
        assert_eq!(config.verbosity, Some(3));
        let _ = fs::remove_dir_all(&cwd);
    }
}

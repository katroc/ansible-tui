use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use chrono::{DateTime, Local};

use crate::app::{RunRecord, RunStatus};
use crate::history::{history_has_legacy_environment_field, load_run_history, save_run_history};

const STORE_DIR: &str = ".ansible-tui";
const STORE_FILE: &str = "history.db";
const LEGACY_STORE_FILE: &str = "runs.db";
const LEGACY_ENV_NOTICE_FILE: &str = "legacy-environment-migration-noted";
const MAX_PERSISTED_RUNS: usize = 500;
const FIELD_SEP: char = '\u{001f}';

pub fn load_runs(cwd: &Path) -> io::Result<Vec<RunRecord>> {
    if !sqlite_available() {
        return load_run_history(cwd);
    }

    ensure_store_dir(cwd)?;
    migrate_legacy_db_filename(cwd)?;
    ensure_schema(cwd)?;
    migrate_from_tsv_if_needed(cwd)?;

    let query = format!(
        "SELECT \
            id || char(31) || \
            hex(playbook) || char(31) || \
            hex(inventory) || char(31) || \
            status || char(31) || \
            started_at || char(31) || \
            COALESCE(finished_at, '') || char(31) || \
            COALESCE(exit_code, '') || char(31) || \
            COALESCE(hex(template_id), '') \
         FROM runs \
         ORDER BY id DESC \
         LIMIT {MAX_PERSISTED_RUNS};"
    );
    let stdout = run_sql_query(cwd, &query)?;

    let mut out = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_fields(line);
        if fields.len() < 7 {
            continue;
        }
        let Ok(id) = fields[0].parse::<u64>() else {
            continue;
        };
        let Ok(playbook_bytes) = decode_hex(fields[1]) else {
            continue;
        };
        let Ok(inventory_bytes) = decode_hex(fields[2]) else {
            continue;
        };
        let Some(status) = parse_status(fields[3]) else {
            continue;
        };
        let Some(started_at) = parse_local_datetime(fields[4]) else {
            continue;
        };
        let finished_at = if fields[5].is_empty() {
            None
        } else {
            parse_local_datetime(fields[5])
        };
        let exit_code = if fields[6].is_empty() {
            None
        } else {
            fields[6].parse::<i32>().ok()
        };
        let template_id = if fields.len() > 7 && !fields[7].is_empty() {
            decode_hex(fields[7])
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
        } else {
            None
        };

        let playbook =
            String::from_utf8(playbook_bytes).map_err(|err| io::Error::other(err.to_string()))?;
        let inventory =
            String::from_utf8(inventory_bytes).map_err(|err| io::Error::other(err.to_string()))?;
        let logs = load_logs_for_run(cwd, id)?;

        out.push(RunRecord {
            id,
            playbook,
            inventory,
            status,
            started_at,
            finished_at,
            exit_code,
            logs,
            template_id,
        });
    }

    Ok(out)
}

pub fn save_run(cwd: &Path, run: &RunRecord) -> io::Result<()> {
    if !sqlite_available() {
        return save_run_legacy_tsv(cwd, run);
    }

    ensure_store_dir(cwd)?;
    migrate_legacy_db_filename(cwd)?;
    ensure_schema(cwd)?;

    let finished_at = run
        .finished_at
        .map(|ts| sql_quote(&ts.to_rfc3339()))
        .unwrap_or_else(|| String::from("NULL"));
    let exit_code = run
        .exit_code
        .map(|v| v.to_string())
        .unwrap_or_else(|| String::from("NULL"));
    let template_id = run
        .template_id
        .as_deref()
        .map(sql_quote)
        .unwrap_or_else(|| String::from("NULL"));
    let mut sql = format!(
        "BEGIN; \
         INSERT INTO runs (id, playbook, inventory, status, started_at, finished_at, exit_code, template_id) \
         VALUES ({id}, {playbook}, {inventory}, {status}, {started}, {finished}, {exit_code}, {template_id}) \
         ON CONFLICT(id) DO UPDATE SET \
            playbook=excluded.playbook, \
            inventory=excluded.inventory, \
            status=excluded.status, \
            started_at=excluded.started_at, \
            finished_at=excluded.finished_at, \
            exit_code=excluded.exit_code, \
            template_id=excluded.template_id; \
         DELETE FROM run_logs WHERE run_id={id};",
        id = run.id,
        playbook = sql_quote(&run.playbook),
        inventory = sql_quote(&run.inventory),
        status = sql_quote(run.status.as_str()),
        started = sql_quote(&run.started_at.to_rfc3339()),
        finished = finished_at,
        exit_code = exit_code,
        template_id = template_id,
    );

    for (seq, line) in run.logs.iter().enumerate() {
        sql.push_str(&format!(
            "INSERT INTO run_logs (run_id, seq, line) VALUES ({}, {}, {});",
            run.id,
            seq,
            sql_quote(line)
        ));
    }

    sql.push_str(&format!(
        "DELETE FROM run_logs WHERE run_id NOT IN (SELECT id FROM runs ORDER BY id DESC LIMIT {MAX_PERSISTED_RUNS}); \
         DELETE FROM runs WHERE id NOT IN (SELECT id FROM runs ORDER BY id DESC LIMIT {MAX_PERSISTED_RUNS}); \
         COMMIT;"
    ));
    run_sql_exec(cwd, &sql)
}

fn save_run_legacy_tsv(cwd: &Path, run: &RunRecord) -> io::Result<()> {
    let mut runs = load_run_history(cwd)?;
    if let Some(existing) = runs.iter_mut().find(|r| r.id == run.id) {
        *existing = run.clone();
    } else {
        runs.push(run.clone());
    }
    runs.sort_by(|a, b| b.id.cmp(&a.id));
    if runs.len() > MAX_PERSISTED_RUNS {
        runs.truncate(MAX_PERSISTED_RUNS);
    }
    save_run_history(cwd, &runs)
}

fn ensure_store_dir(cwd: &Path) -> io::Result<()> {
    fs::create_dir_all(cwd.join(STORE_DIR))
}

fn migrate_legacy_db_filename(cwd: &Path) -> io::Result<()> {
    let new_path = store_path(cwd);
    if new_path.is_file() {
        return Ok(());
    }

    let legacy_path = legacy_store_path(cwd);
    if !legacy_path.is_file() {
        return Ok(());
    }

    fs::rename(legacy_path, new_path)
}

fn ensure_schema(cwd: &Path) -> io::Result<()> {
    run_sql_exec(
        cwd,
        "PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS runs (
            id INTEGER PRIMARY KEY,
            playbook TEXT NOT NULL,
            inventory TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT NOT NULL,
            finished_at TEXT,
            exit_code INTEGER,
            template_id TEXT
         );
         CREATE TABLE IF NOT EXISTS run_logs (
            run_id INTEGER NOT NULL,
            seq INTEGER NOT NULL,
            line TEXT NOT NULL,
            PRIMARY KEY (run_id, seq)
         );
         CREATE INDEX IF NOT EXISTS idx_run_logs_run_id_seq ON run_logs(run_id, seq);",
    )?;
    migrate_add_template_column(cwd)
}

fn migrate_add_template_column(cwd: &Path) -> io::Result<()> {
    let info = run_sql_query(cwd, "PRAGMA table_info(runs);")?;
    if !info.contains("template_id") {
        run_sql_exec(cwd, "ALTER TABLE runs ADD COLUMN template_id TEXT;")?;
    }
    Ok(())
}

fn migrate_from_tsv_if_needed(cwd: &Path) -> io::Result<()> {
    let count_stdout = run_sql_query(cwd, "SELECT COUNT(*) FROM runs;")?;
    let count = count_stdout.trim().parse::<usize>().unwrap_or(0);
    if count > 0 {
        return Ok(());
    }

    let legacy_runs = load_run_history(cwd)?;
    if legacy_runs.is_empty() {
        return Ok(());
    }
    for run in legacy_runs.into_iter().take(MAX_PERSISTED_RUNS) {
        save_run(cwd, &run)?;
    }
    Ok(())
}

fn load_logs_for_run(cwd: &Path, run_id: u64) -> io::Result<Vec<String>> {
    let stdout = run_sql_query(
        cwd,
        &format!(
            "SELECT seq || char(31) || hex(line)
             FROM run_logs
             WHERE run_id={}
             ORDER BY seq ASC;",
            run_id
        ),
    )?;

    let mut out = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_fields(line);
        if fields.len() != 2 {
            continue;
        }
        let Ok(bytes) = decode_hex(fields[1]) else {
            continue;
        };
        if let Ok(text) = String::from_utf8(bytes) {
            out.push(text);
        }
    }
    Ok(out)
}

fn run_sql_exec(cwd: &Path, sql: &str) -> io::Result<()> {
    let output = Command::new("sqlite3")
        .arg("-batch")
        .arg(store_path(cwd))
        .arg(sql)
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

fn run_sql_query(cwd: &Path, sql: &str) -> io::Result<String> {
    let output = Command::new("sqlite3")
        .arg("-batch")
        .arg(store_path(cwd))
        .arg(sql)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn store_path(cwd: &Path) -> PathBuf {
    cwd.join(STORE_DIR).join(STORE_FILE)
}

fn legacy_store_path(cwd: &Path) -> PathBuf {
    cwd.join(STORE_DIR).join(LEGACY_STORE_FILE)
}

pub fn take_legacy_environment_migration_notice(cwd: &Path) -> io::Result<Option<String>> {
    let note_path = legacy_environment_notice_path(cwd);
    if note_path.is_file() {
        return Ok(None);
    }

    let has_legacy_environment = sqlite_store_has_legacy_environment_column(cwd)?
        || history_has_legacy_environment_field(cwd)?;
    if !has_legacy_environment {
        return Ok(None);
    }

    ensure_store_dir(cwd)?;
    fs::write(note_path, "acknowledged\n")?;
    Ok(Some(String::from(
        "Migration note: historical runs remain compatible, but legacy environment labels were removed from history and templates.",
    )))
}

fn sqlite_store_has_legacy_environment_column(cwd: &Path) -> io::Result<bool> {
    if !sqlite_available() {
        return Ok(false);
    }
    let path = store_path(cwd);
    if !path.is_file() {
        return Ok(false);
    }
    let info = run_sql_query(cwd, "PRAGMA table_info(runs);")?;
    Ok(info.lines().any(|line| {
        line.split('|')
            .nth(1)
            .map(|name| name == "environment")
            .unwrap_or(false)
    }))
}

fn legacy_environment_notice_path(cwd: &Path) -> PathBuf {
    cwd.join(STORE_DIR).join(LEGACY_ENV_NOTICE_FILE)
}

fn sql_quote(value: &str) -> String {
    let mut quoted = String::from("'");
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push('\'');
            quoted.push('\'');
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn split_fields(line: &str) -> Vec<&str> {
    line.split(FIELD_SEP).collect()
}

fn parse_status(raw: &str) -> Option<RunStatus> {
    match raw {
        "running" => Some(RunStatus::Running),
        "succeeded" => Some(RunStatus::Succeeded),
        "failed" => Some(RunStatus::Failed),
        _ => None,
    }
}

fn parse_local_datetime(raw: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|ts| ts.with_timezone(&Local))
}

fn decode_hex(raw: &str) -> io::Result<Vec<u8>> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    if raw.len() % 2 != 0 {
        return Err(io::Error::other("invalid hex length"));
    }
    let mut out = Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = from_hex_digit(bytes[i]).ok_or_else(|| io::Error::other("invalid hex"))?;
        let lo = from_hex_digit(bytes[i + 1]).ok_or_else(|| io::Error::other("invalid hex"))?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn from_hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

fn sqlite_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("sqlite3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

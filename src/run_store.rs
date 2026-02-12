use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use rusqlite::params;

use crate::app::{RunRecord, RunStatus};
use crate::db::{open_db, sqlite_to_io};

const STORE_DIR: &str = ".ansible-tui";
const STORE_FILE: &str = "history.db";
const LEGACY_STORE_FILE: &str = "runs.db";
const LEGACY_TSV_FILE: &str = "runs.tsv";
const LEGACY_ENV_NOTICE_FILE: &str = "legacy-environment-migration-noted";
const MAX_PERSISTED_RUNS: usize = 500;

pub fn load_runs(cwd: &Path) -> io::Result<Vec<RunRecord>> {
    migrate_legacy_db_filename(cwd)?;
    let conn = open_history_db(cwd)?;
    migrate_from_tsv_if_needed(cwd, &conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT id, playbook, inventory, status, started_at, finished_at, exit_code, template_id \
             FROM runs ORDER BY id DESC LIMIT ?1",
        )
        .map_err(sqlite_to_io)?;

    let rows = stmt
        .query_map(params![MAX_PERSISTED_RUNS as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i32>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(sqlite_to_io)?;

    let mut out = Vec::new();
    for row in rows {
        let (
            id,
            playbook,
            inventory,
            status_str,
            started_str,
            finished_str,
            exit_code,
            template_id,
        ) = row.map_err(sqlite_to_io)?;
        let Some(status) = parse_status(&status_str) else {
            continue;
        };
        let Some(started_at) = parse_local_datetime(&started_str) else {
            continue;
        };
        let finished_at = finished_str.as_deref().and_then(parse_local_datetime);
        let logs = load_logs_for_run(&conn, id as u64)?;

        out.push(RunRecord {
            id: id as u64,
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
    let conn = open_history_db(cwd)?;

    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;

    tx.execute(
        "INSERT INTO runs (id, playbook, inventory, status, started_at, finished_at, exit_code, template_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(id) DO UPDATE SET \
            playbook=excluded.playbook, \
            inventory=excluded.inventory, \
            status=excluded.status, \
            started_at=excluded.started_at, \
            finished_at=excluded.finished_at, \
            exit_code=excluded.exit_code, \
            template_id=excluded.template_id",
        params![
            run.id as i64,
            run.playbook,
            run.inventory,
            run.status.as_str(),
            run.started_at.to_rfc3339(),
            run.finished_at.map(|ts| ts.to_rfc3339()),
            run.exit_code,
            run.template_id,
        ],
    )
    .map_err(sqlite_to_io)?;

    tx.execute(
        "DELETE FROM run_logs WHERE run_id=?1",
        params![run.id as i64],
    )
    .map_err(sqlite_to_io)?;

    {
        let mut insert_log = tx
            .prepare("INSERT INTO run_logs (run_id, seq, line) VALUES (?1, ?2, ?3)")
            .map_err(sqlite_to_io)?;
        for (seq, line) in run.logs.iter().enumerate() {
            insert_log
                .execute(params![run.id as i64, seq as i64, line])
                .map_err(sqlite_to_io)?;
        }
    }

    tx.execute(
        "DELETE FROM run_logs WHERE run_id NOT IN (SELECT id FROM runs ORDER BY id DESC LIMIT ?1)",
        params![MAX_PERSISTED_RUNS as i64],
    )
    .map_err(sqlite_to_io)?;
    tx.execute(
        "DELETE FROM runs WHERE id NOT IN (SELECT id FROM runs ORDER BY id DESC LIMIT ?1)",
        params![MAX_PERSISTED_RUNS as i64],
    )
    .map_err(sqlite_to_io)?;

    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

pub fn take_legacy_environment_migration_notice(cwd: &Path) -> io::Result<Option<String>> {
    let note_path = legacy_environment_notice_path(cwd);
    if note_path.is_file() {
        return Ok(None);
    }

    let has_legacy =
        sqlite_store_has_legacy_environment_column(cwd)? || tsv_has_legacy_environment_field(cwd)?;
    if !has_legacy {
        return Ok(None);
    }

    fs::create_dir_all(cwd.join(STORE_DIR))?;
    fs::write(note_path, "acknowledged\n")?;
    Ok(Some(String::from(
        "Migration note: historical runs remain compatible, but legacy environment labels were removed from history and templates.",
    )))
}

/// Migrate history from directories created with the old unstable `DefaultHasher`.
/// If the stable-hash directory has no db yet, scan sibling directories for one
/// that contains a `history.db` and rename it to the correct location.
pub fn migrate_unstable_hash_history(stable_root: &Path) {
    let db_path = stable_root.join(STORE_DIR).join(STORE_FILE);
    if db_path.is_file() {
        return;
    }
    let Some(parent) = stable_root.parent() else {
        return;
    };
    if !parent.is_dir() {
        return;
    }
    let stable_name = stable_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let mut candidate: Option<PathBuf> = None;
    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if !entry_path.is_dir() {
                continue;
            }
            let name = entry_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name == stable_name {
                continue;
            }
            if entry_path.join(STORE_DIR).join(STORE_FILE).is_file() {
                if candidate.is_some() {
                    // Multiple candidates – ambiguous, skip migration
                    return;
                }
                candidate = Some(entry_path);
            }
        }
    }
    if let Some(old_dir) = candidate {
        let _ = fs::rename(&old_dir, stable_root);
    }
}

// --- private helpers ---

fn open_history_db(cwd: &Path) -> io::Result<rusqlite::Connection> {
    let conn = open_db(&store_path(cwd))?;
    ensure_schema(&conn)?;
    Ok(conn)
}

fn ensure_schema(conn: &rusqlite::Connection) -> io::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS runs (
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
    )
    .map_err(sqlite_to_io)?;
    migrate_add_template_column(conn)
}

fn migrate_add_template_column(conn: &rusqlite::Connection) -> io::Result<()> {
    let has_col = conn
        .prepare("PRAGMA table_info(runs)")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))
                .map(|rows| {
                    rows.filter_map(|r| r.ok())
                        .any(|name| name == "template_id")
                })
        })
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch("ALTER TABLE runs ADD COLUMN template_id TEXT;")
            .map_err(sqlite_to_io)?;
    }
    Ok(())
}

fn migrate_legacy_db_filename(cwd: &Path) -> io::Result<()> {
    let new_path = store_path(cwd);
    if new_path.is_file() {
        return Ok(());
    }
    let legacy_path = cwd.join(STORE_DIR).join(LEGACY_STORE_FILE);
    if !legacy_path.is_file() {
        return Ok(());
    }
    fs::rename(legacy_path, new_path)
}

fn migrate_from_tsv_if_needed(cwd: &Path, conn: &rusqlite::Connection) -> io::Result<()> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM runs", [], |row| row.get(0))
        .map_err(sqlite_to_io)?;
    if count > 0 {
        return Ok(());
    }

    let tsv_path = cwd.join(STORE_DIR).join(LEGACY_TSV_FILE);
    let data = match fs::read_to_string(tsv_path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    let legacy_runs = parse_tsv_runs(&data);
    if legacy_runs.is_empty() {
        return Ok(());
    }

    // Save via the connection directly to avoid recursive open
    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    for run in legacy_runs.into_iter().take(MAX_PERSISTED_RUNS) {
        tx.execute(
            "INSERT OR IGNORE INTO runs (id, playbook, inventory, status, started_at, finished_at, exit_code, template_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.id as i64,
                run.playbook,
                run.inventory,
                run.status.as_str(),
                run.started_at.to_rfc3339(),
                run.finished_at.map(|ts| ts.to_rfc3339()),
                run.exit_code,
                run.template_id,
            ],
        )
        .map_err(sqlite_to_io)?;
    }
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

fn load_logs_for_run(conn: &rusqlite::Connection, run_id: u64) -> io::Result<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT line FROM run_logs WHERE run_id=?1 ORDER BY seq ASC")
        .map_err(sqlite_to_io)?;
    let rows = stmt
        .query_map(params![run_id as i64], |row| row.get::<_, String>(0))
        .map_err(sqlite_to_io)?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sqlite_to_io)?);
    }
    Ok(out)
}

fn store_path(cwd: &Path) -> PathBuf {
    cwd.join(STORE_DIR).join(STORE_FILE)
}

fn legacy_environment_notice_path(cwd: &Path) -> PathBuf {
    cwd.join(STORE_DIR).join(LEGACY_ENV_NOTICE_FILE)
}

fn sqlite_store_has_legacy_environment_column(cwd: &Path) -> io::Result<bool> {
    let path = store_path(cwd);
    if !path.is_file() {
        return Ok(false);
    }
    let conn = open_db(&path)?;
    let has_col = conn
        .prepare("PRAGMA table_info(runs)")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))
                .map(|rows| {
                    rows.filter_map(|r| r.ok())
                        .any(|name| name == "environment")
                })
        })
        .unwrap_or(false);
    Ok(has_col)
}

fn tsv_has_legacy_environment_field(cwd: &Path) -> io::Result<bool> {
    let tsv_path = cwd.join(STORE_DIR).join(LEGACY_TSV_FILE);
    let data = match fs::read_to_string(tsv_path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.split('\t').count() >= 9 {
            return Ok(true);
        }
    }
    Ok(false)
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

// --- TSV parsing for legacy migration ---

fn parse_tsv_runs(data: &str) -> Vec<RunRecord> {
    let mut runs = Vec::new();
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(run) = parse_tsv_line(line) {
            runs.push(run);
        }
    }
    runs.sort_by(|a, b| b.id.cmp(&a.id));
    runs
}

fn parse_tsv_line(line: &str) -> Option<RunRecord> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() < 7 {
        return None;
    }
    let id = fields[0].parse::<u64>().ok()?;
    let status = parse_status(fields[1])?;
    let started_at = parse_local_datetime(fields[2])?;
    let finished_at = if fields[3].is_empty() {
        None
    } else {
        parse_local_datetime(fields[3])
    };
    let exit_code = if fields[4].is_empty() {
        None
    } else {
        fields[4].parse::<i32>().ok()
    };
    let playbook = tsv_unescape(fields[5]);
    let inventory = tsv_unescape(fields[6]);
    let template_id = fields.get(7).and_then(|v| {
        let s = tsv_unescape(v).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    });

    Some(RunRecord {
        id,
        playbook,
        inventory,
        status,
        started_at,
        finished_at,
        exit_code,
        logs: Vec::new(),
        template_id,
    })
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

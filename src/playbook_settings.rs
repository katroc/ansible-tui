use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use rusqlite::params;

use crate::db::{open_db, project_db_path, sqlite_to_io};

#[derive(Debug, Clone)]
pub struct PlaybookSettings {
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
}

impl Default for PlaybookSettings {
    fn default() -> Self {
        Self {
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
        }
    }
}

const SETTINGS_DIR: &str = ".ansible-tui";
const LEGACY_SETTINGS_FILE: &str = "playbook_settings.tsv";
const LEGACY_SETTINGS_FILE_2: &str = "playbook_templates.tsv";

pub fn load_playbook_settings(cwd: &Path) -> io::Result<BTreeMap<String, PlaybookSettings>> {
    let conn = open_settings_db(cwd)?;
    migrate_from_tsv_if_needed(cwd, &conn)?;

    let mut stmt = conn
        .prepare(
            "SELECT playbook_path, \"check\", diff, become_enabled, verbosity, \
             forks, timeout, \"limit\", tags, extra_vars, extra_args, \
             ssh_private_key_file, ssh_private_key_inline \
             FROM playbook_settings ORDER BY playbook_path ASC",
        )
        .map_err(sqlite_to_io)?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? != 0,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)? != 0,
                row.get::<_, i64>(4)?.min(4) as u8,
                row.get::<_, Option<i64>>(5)?.map(|v| v as u16),
                row.get::<_, Option<i64>>(6)?.map(|v| v as u16),
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
            ))
        })
        .map_err(sqlite_to_io)?;

    let mut out = BTreeMap::new();
    for row in rows {
        let (path, check, diff, become_enabled, verbosity, forks, timeout, limit, tags, extra_vars, extra_args, ssh_key_file, ssh_key_inline) =
            row.map_err(sqlite_to_io)?;
        out.insert(
            path,
            PlaybookSettings {
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
                ssh_private_key_file: ssh_key_file,
                ssh_private_key_inline: ssh_key_inline,
            },
        );
    }
    Ok(out)
}

pub fn save_playbook_settings(
    cwd: &Path,
    settings: &BTreeMap<String, PlaybookSettings>,
) -> io::Result<()> {
    let conn = open_settings_db(cwd)?;
    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    tx.execute("DELETE FROM playbook_settings", [])
        .map_err(sqlite_to_io)?;

    let mut insert = tx
        .prepare(
            "INSERT INTO playbook_settings \
             (playbook_path, \"check\", diff, become_enabled, verbosity, \
              forks, timeout, \"limit\", tags, extra_vars, extra_args, \
              ssh_private_key_file, ssh_private_key_inline) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        )
        .map_err(sqlite_to_io)?;

    for (playbook, s) in settings {
        insert
            .execute(params![
                playbook,
                s.check as i64,
                s.diff as i64,
                s.become_enabled as i64,
                s.verbosity as i64,
                s.forks.map(|v| v as i64),
                s.timeout.map(|v| v as i64),
                s.limit,
                s.tags,
                s.extra_vars,
                s.extra_args,
                s.ssh_private_key_file,
                s.ssh_private_key_inline,
            ])
            .map_err(sqlite_to_io)?;
    }

    drop(insert);
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

pub fn cycle_u16(current: Option<u16>, presets: &[Option<u16>], delta: i8) -> Option<u16> {
    if presets.is_empty() {
        return current;
    }
    let pos = presets.iter().position(|x| *x == current).unwrap_or(0) as isize;
    let step = if delta < 0 { -1 } else { 1 };
    let mut next = pos + step;
    if next < 0 {
        next = presets.len() as isize - 1;
    } else if next >= presets.len() as isize {
        next = 0;
    }
    presets[next as usize]
}

// --- private helpers ---

fn open_settings_db(cwd: &Path) -> io::Result<rusqlite::Connection> {
    let conn = open_db(&project_db_path(cwd))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS playbook_settings (
            playbook_path TEXT PRIMARY KEY,
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
            ssh_private_key_inline TEXT
         )",
    )
    .map_err(sqlite_to_io)?;
    Ok(conn)
}

fn migrate_from_tsv_if_needed(cwd: &Path, conn: &rusqlite::Connection) -> io::Result<()> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM playbook_settings", [], |row| {
            row.get(0)
        })
        .map_err(sqlite_to_io)?;
    if count > 0 {
        return Ok(());
    }

    // Try both legacy filenames
    let data = {
        let primary = cwd.join(SETTINGS_DIR).join(LEGACY_SETTINGS_FILE);
        match fs::read_to_string(primary) {
            Ok(data) => data,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                let secondary = cwd.join(SETTINGS_DIR).join(LEGACY_SETTINGS_FILE_2);
                match fs::read_to_string(secondary) {
                    Ok(data) => data,
                    Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
                    Err(err) => return Err(err),
                }
            }
            Err(err) => return Err(err),
        }
    };

    let tx = conn.unchecked_transaction().map_err(sqlite_to_io)?;
    let mut insert = tx
        .prepare(
            "INSERT OR IGNORE INTO playbook_settings \
             (playbook_path, \"check\", diff, become_enabled, verbosity, \
              forks, timeout, \"limit\", tags, extra_vars, extra_args, \
              ssh_private_key_file, ssh_private_key_inline) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        )
        .map_err(sqlite_to_io)?;

    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some((path, s)) = parse_tsv_line(line) {
            insert
                .execute(params![
                    path,
                    s.check as i64,
                    s.diff as i64,
                    s.become_enabled as i64,
                    s.verbosity as i64,
                    s.forks.map(|v| v as i64),
                    s.timeout.map(|v| v as i64),
                    s.limit,
                    s.tags,
                    s.extra_vars,
                    s.extra_args,
                    s.ssh_private_key_file,
                    s.ssh_private_key_inline,
                ])
                .map_err(sqlite_to_io)?;
        }
    }

    drop(insert);
    tx.commit().map_err(sqlite_to_io)?;
    Ok(())
}

// --- TSV legacy parsing ---

fn parse_tsv_line(line: &str) -> Option<(String, PlaybookSettings)> {
    let mut fields = line.split('\t');
    let playbook = tsv_unescape(fields.next()?);
    let check = parse_bool(fields.next().unwrap_or_default());
    let diff = parse_bool(fields.next().unwrap_or_default());
    let become_enabled = parse_bool(fields.next().unwrap_or_default());
    let verbosity = fields
        .next()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(0)
        .min(4);
    let forks = parse_opt_u16(fields.next().unwrap_or_default());
    let timeout = parse_opt_u16(fields.next().unwrap_or_default());
    let limit = parse_opt_string(fields.next().unwrap_or_default());
    let tags = parse_opt_string(fields.next().unwrap_or_default());
    let extra_vars = parse_opt_string(fields.next().unwrap_or_default());
    let extra_args = parse_opt_string(fields.next().unwrap_or_default());
    let ssh_private_key_file = parse_opt_string(fields.next().unwrap_or_default());
    let ssh_private_key_inline = parse_opt_string(fields.next().unwrap_or_default());

    Some((
        playbook,
        PlaybookSettings {
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
        },
    ))
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

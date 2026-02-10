use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::app::{RunRecord, RunStatus};

const HISTORY_DIR: &str = ".ansible-tui";
const HISTORY_FILE: &str = "runs.tsv";
const MAX_PERSISTED_RUNS: usize = 500;

pub fn load_run_history(cwd: &Path) -> io::Result<Vec<RunRecord>> {
    let path = history_file(cwd);
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };

    let mut runs = Vec::new();
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(run) = parse_line(line) {
            runs.push(run);
        }
    }

    runs.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(runs)
}

pub fn save_run_history(cwd: &Path, runs: &[RunRecord]) -> io::Result<()> {
    let dir = cwd.join(HISTORY_DIR);
    fs::create_dir_all(&dir)?;
    let path = history_file(cwd);

    let mut out = String::new();
    for run in runs.iter().take(MAX_PERSISTED_RUNS) {
        out.push_str(&format_line(run));
        out.push('\n');
    }
    fs::write(path, out)
}

fn history_file(cwd: &Path) -> PathBuf {
    cwd.join(HISTORY_DIR).join(HISTORY_FILE)
}

fn format_line(run: &RunRecord) -> String {
    let status = run.status.as_str();
    let started = run.started_at.to_rfc3339();
    let finished = run
        .finished_at
        .map(|ts| ts.to_rfc3339())
        .unwrap_or_default();
    let exit_code = run.exit_code.map(|v| v.to_string()).unwrap_or_default();
    let playbook = escape(&run.playbook);
    let inventory = escape(&run.inventory);

    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        run.id, status, started, finished, exit_code, playbook, inventory
    )
}

fn parse_line(line: &str) -> Option<RunRecord> {
    let mut fields = line.split('\t');
    let id = fields.next()?.parse::<u64>().ok()?;
    let status = parse_status(fields.next()?)?;
    let started_at = parse_local_datetime(fields.next()?)?;
    let finished_at = parse_optional_datetime(fields.next().unwrap_or_default());
    let exit_code = parse_optional_i32(fields.next().unwrap_or_default());
    let playbook = unescape(fields.next().unwrap_or_default());
    let inventory = unescape(fields.next().unwrap_or_default());

    Some(RunRecord {
        id,
        playbook,
        inventory,
        status,
        started_at,
        finished_at,
        exit_code,
        logs: Vec::new(),
    })
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

fn parse_optional_datetime(raw: &str) -> Option<DateTime<Local>> {
    if raw.is_empty() {
        None
    } else {
        parse_local_datetime(raw)
    }
}

fn parse_optional_i32(raw: &str) -> Option<i32> {
    if raw.is_empty() {
        None
    } else {
        raw.parse::<i32>().ok()
    }
}

fn escape(raw: &str) -> String {
    let mut escaped = String::new();
    for ch in raw.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn unescape(raw: &str) -> String {
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

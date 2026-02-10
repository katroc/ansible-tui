use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
        }
    }
}

const SETTINGS_DIR: &str = ".ansible-tui";
const SETTINGS_FILE: &str = "playbook_settings.tsv";
const LEGACY_SETTINGS_FILE: &str = "playbook_templates.tsv";

pub fn load_playbook_settings(cwd: &Path) -> io::Result<BTreeMap<String, PlaybookSettings>> {
    let path = settings_file(cwd);
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let legacy = cwd.join(SETTINGS_DIR).join(LEGACY_SETTINGS_FILE);
            match fs::read_to_string(legacy) {
                Ok(data) => data,
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
                Err(err) => return Err(err),
            }
        }
        Err(err) => return Err(err),
    };

    let mut out = BTreeMap::new();
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some((key, settings)) = parse_line(line) {
            out.insert(key, settings);
        }
    }
    Ok(out)
}

pub fn save_playbook_settings(
    cwd: &Path,
    settings: &BTreeMap<String, PlaybookSettings>,
) -> io::Result<()> {
    let dir = cwd.join(SETTINGS_DIR);
    fs::create_dir_all(&dir)?;
    let path = settings_file(cwd);

    let mut out = String::new();
    for (playbook, setting) in settings {
        out.push_str(&format_line(playbook, setting));
        out.push('\n');
    }
    fs::write(path, out)
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

fn settings_file(cwd: &Path) -> PathBuf {
    cwd.join(SETTINGS_DIR).join(SETTINGS_FILE)
}

fn format_line(playbook: &str, settings: &PlaybookSettings) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        escape(playbook),
        bool_to_u8(settings.check),
        bool_to_u8(settings.diff),
        bool_to_u8(settings.become_enabled),
        settings.verbosity,
        settings.forks.map(|v| v.to_string()).unwrap_or_default(),
        settings.timeout.map(|v| v.to_string()).unwrap_or_default(),
        escape_opt(&settings.limit),
        escape_opt(&settings.tags),
        escape_opt(&settings.extra_vars),
        escape_opt(&settings.extra_args),
    )
}

fn parse_line(line: &str) -> Option<(String, PlaybookSettings)> {
    let mut fields = line.split('\t');
    let playbook = unescape(fields.next()?);
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
        },
    ))
}

fn parse_bool(raw: &str) -> bool {
    matches!(raw.trim(), "1" | "true" | "yes")
}

fn bool_to_u8(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

fn parse_opt_u16(raw: &str) -> Option<u16> {
    let raw = raw.trim();
    if raw.is_empty() {
        None
    } else {
        raw.parse::<u16>().ok()
    }
}

fn escape_opt(value: &Option<String>) -> String {
    value.as_deref().map(escape).unwrap_or_default()
}

fn parse_opt_string(raw: &str) -> Option<String> {
    let raw = unescape(raw);
    let raw = raw.trim().to_string();
    if raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

fn escape(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
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

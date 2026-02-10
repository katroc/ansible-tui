use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AnsibleCfgSettings {
    pub interpreter_python: Option<String>,
    pub forks: Option<u16>,
    pub timeout: Option<u16>,
    pub verbosity: u8,
    pub host_key_checking: bool,
    pub stdout_callback: Option<String>,
    pub retry_files_enabled: bool,
    pub retry_files_save_path: Option<String>,
    pub remote_user: Option<String>,
    pub private_key_file: Option<String>,
    pub pipelining: bool,
}

impl Default for AnsibleCfgSettings {
    fn default() -> Self {
        Self {
            interpreter_python: None,
            forks: None,
            timeout: None,
            verbosity: 0,
            host_key_checking: true,
            stdout_callback: None,
            retry_files_enabled: false,
            retry_files_save_path: None,
            remote_user: None,
            private_key_file: None,
            pipelining: false,
        }
    }
}

const ANSIBLE_CFG_FILE: &str = "ansible.cfg";

pub fn load_ansible_cfg_settings(cwd: &Path) -> io::Result<AnsibleCfgSettings> {
    let path = ansible_cfg_path(cwd);
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(AnsibleCfgSettings::default())
        }
        Err(err) => return Err(err),
    };

    let mut out = AnsibleCfgSettings::default();
    let mut section = String::new();

    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        match (section.as_str(), key.as_str()) {
            ("defaults", "interpreter_python") => out.interpreter_python = parse_opt_string(value),
            ("defaults", "forks") => out.forks = parse_opt_u16(value),
            ("defaults", "timeout") => out.timeout = parse_opt_u16(value),
            ("defaults", "verbosity") => {
                if let Some(v) = parse_opt_u8(value) {
                    out.verbosity = v.min(4);
                }
            }
            ("defaults", "host_key_checking") => {
                if let Some(v) = parse_opt_bool(value) {
                    out.host_key_checking = v;
                }
            }
            ("defaults", "stdout_callback") => out.stdout_callback = parse_opt_string(value),
            ("defaults", "retry_files_enabled") => {
                if let Some(v) = parse_opt_bool(value) {
                    out.retry_files_enabled = v;
                }
            }
            ("defaults", "retry_files_save_path") => {
                out.retry_files_save_path = parse_opt_string(value)
            }
            ("defaults", "remote_user") => out.remote_user = parse_opt_string(value),
            ("defaults", "private_key_file") => out.private_key_file = parse_opt_string(value),
            ("defaults", "pipelining") | ("connection", "pipelining") => {
                if let Some(v) = parse_opt_bool(value) {
                    out.pipelining = v;
                }
            }
            _ => {}
        }
    }

    Ok(out)
}

pub fn save_ansible_cfg_settings(cwd: &Path, settings: &AnsibleCfgSettings) -> io::Result<()> {
    let path = ansible_cfg_path(cwd);
    let mut lines = match fs::read_to_string(&path) {
        Ok(data) => data.lines().map(|l| l.to_string()).collect::<Vec<_>>(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(err) => return Err(err),
    };

    upsert_key(
        &mut lines,
        "defaults",
        "interpreter_python",
        settings.interpreter_python.as_deref(),
    );
    let forks = settings.forks.map(|v| v.to_string());
    let timeout = settings.timeout.map(|v| v.to_string());
    let verbosity = settings.verbosity.to_string();
    upsert_key(&mut lines, "defaults", "forks", forks.as_deref());
    upsert_key(&mut lines, "defaults", "timeout", timeout.as_deref());
    upsert_key(
        &mut lines,
        "defaults",
        "verbosity",
        Some(verbosity.as_str()),
    );
    upsert_key(
        &mut lines,
        "defaults",
        "host_key_checking",
        Some(bool_as_ini(settings.host_key_checking)),
    );
    upsert_key(
        &mut lines,
        "defaults",
        "stdout_callback",
        settings.stdout_callback.as_deref(),
    );
    upsert_key(
        &mut lines,
        "defaults",
        "retry_files_enabled",
        Some(bool_as_ini(settings.retry_files_enabled)),
    );
    upsert_key(
        &mut lines,
        "defaults",
        "retry_files_save_path",
        settings.retry_files_save_path.as_deref(),
    );
    upsert_key(
        &mut lines,
        "defaults",
        "remote_user",
        settings.remote_user.as_deref(),
    );
    upsert_key(
        &mut lines,
        "defaults",
        "private_key_file",
        settings.private_key_file.as_deref(),
    );
    upsert_key(
        &mut lines,
        "defaults",
        "pipelining",
        Some(bool_as_ini(settings.pipelining)),
    );

    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    fs::write(path, out)
}

fn upsert_key(lines: &mut Vec<String>, section: &str, key: &str, value: Option<&str>) {
    let mut in_section = false;
    let mut section_start = None;
    let mut section_end = lines.len();
    let mut key_idx = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if is_section_header(trimmed) {
            let current = trimmed[1..trimmed.len() - 1].trim().to_ascii_lowercase();
            if in_section {
                section_end = idx;
                break;
            }
            if current == section {
                in_section = true;
                section_start = Some(idx);
            }
            continue;
        }
        if !in_section || trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';')
        {
            continue;
        }
        if let Some((k, _)) = trimmed.split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                key_idx = Some(idx);
            }
        }
    }

    if !in_section {
        let Some(value) = value else {
            return;
        };
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("[{section}]"));
        lines.push(format!("{key} = {value}"));
        return;
    }

    if let Some(idx) = key_idx {
        if let Some(value) = value {
            lines[idx] = format!("{key} = {value}");
        } else {
            lines.remove(idx);
        }
        return;
    }

    let Some(value) = value else {
        return;
    };
    let insert_idx = section_end.max(section_start.unwrap_or(0) + 1);
    lines.insert(insert_idx, format!("{key} = {value}"));
}

fn is_section_header(value: &str) -> bool {
    value.starts_with('[') && value.ends_with(']') && value.len() > 2
}

fn bool_as_ini(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn parse_opt_string(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_opt_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_opt_u8(raw: &str) -> Option<u8> {
    raw.trim().parse::<u8>().ok()
}

fn parse_opt_u16(raw: &str) -> Option<u16> {
    raw.trim().parse::<u16>().ok()
}

fn ansible_cfg_path(cwd: &Path) -> PathBuf {
    cwd.join(ANSIBLE_CFG_FILE)
}

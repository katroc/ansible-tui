use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
}

const CONFIG_DIR: &str = ".ansible-tui";
const CONFIG_FILE: &str = "config.env";

pub fn load_app_config(cwd: &Path) -> io::Result<AppConfig> {
    let path = config_path(cwd);
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(AppConfig::default()),
        Err(err) => return Err(err),
    };

    let mut config = AppConfig::default();
    for line in data.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "ansible_bin" => config.ansible_bin = parse_opt_string(value),
                "check" => config.check = parse_opt_bool(value),
                "diff" => config.diff = parse_opt_bool(value),
                "become_enabled" => config.become_enabled = parse_opt_bool(value),
                "verbosity" => config.verbosity = parse_opt_u8(value),
                "forks" => config.forks = parse_opt_u16(value),
                "timeout" => config.timeout = parse_opt_u16(value),
                "limit" => config.limit = parse_opt_string(value),
                "tags" => config.tags = parse_opt_string(value),
                "extra_vars" => config.extra_vars = parse_opt_string(value),
                "extra_args" => config.extra_args = parse_opt_string(value),
                _ => {}
            }
        }
    }

    Ok(config)
}

pub fn save_app_config(cwd: &Path, config: &AppConfig) -> io::Result<()> {
    let dir = cwd.join(CONFIG_DIR);
    fs::create_dir_all(&dir)?;
    let path = config_path(cwd);

    let mut out = String::new();
    push_line_opt_string(&mut out, "ansible_bin", &config.ansible_bin);
    push_line_opt_bool(&mut out, "check", config.check);
    push_line_opt_bool(&mut out, "diff", config.diff);
    push_line_opt_bool(&mut out, "become_enabled", config.become_enabled);
    push_line_opt_u8(&mut out, "verbosity", config.verbosity);
    push_line_opt_u16(&mut out, "forks", config.forks);
    push_line_opt_u16(&mut out, "timeout", config.timeout);
    push_line_opt_string(&mut out, "limit", &config.limit);
    push_line_opt_string(&mut out, "tags", &config.tags);
    push_line_opt_string(&mut out, "extra_vars", &config.extra_vars);
    push_line_opt_string(&mut out, "extra_args", &config.extra_args);
    fs::write(path, out)
}

fn config_path(cwd: &Path) -> PathBuf {
    cwd.join(CONFIG_DIR).join(CONFIG_FILE)
}

fn escape_value(value: &str) -> String {
    value.replace('\n', "").replace('\r', "")
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

fn push_line_opt_string(out: &mut String, key: &str, value: &Option<String>) {
    if let Some(value) = value {
        out.push_str(key);
        out.push('=');
        out.push_str(&escape_value(value));
        out.push('\n');
    }
}

fn push_line_opt_bool(out: &mut String, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        out.push_str(key);
        out.push('=');
        out.push_str(if value { "1" } else { "0" });
        out.push('\n');
    }
}

fn push_line_opt_u8(out: &mut String, key: &str, value: Option<u8>) {
    if let Some(value) = value {
        out.push_str(key);
        out.push('=');
        out.push_str(&value.to_string());
        out.push('\n');
    }
}

fn push_line_opt_u16(out: &mut String, key: &str, value: Option<u16>) {
    if let Some(value) = value {
        out.push_str(key);
        out.push('=');
        out.push_str(&value.to_string());
        out.push('\n');
    }
}

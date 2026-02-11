use std::fs;
use std::io;
use std::path::Path;

use crate::run::RunOptions;

#[derive(Debug, Clone)]
pub struct JobTemplate {
    pub id: String,
    pub name: String,
    pub playbook: String,
    pub inventory: String,
    pub environment: Option<String>,
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

impl JobTemplate {
    pub fn new(name: &str) -> Self {
        Self {
            id: generate_id(name),
            name: name.to_string(),
            playbook: String::new(),
            inventory: String::new(),
            environment: None,
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
const TEMPLATES_FILE: &str = "job_templates.tsv";

pub fn load_job_templates(cwd: &Path) -> io::Result<Vec<JobTemplate>> {
    let path = cwd.join(SETTINGS_DIR).join(TEMPLATES_FILE);
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };

    let mut out = Vec::new();
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(template) = parse_line(line) {
            out.push(template);
        }
    }
    Ok(out)
}

pub fn save_job_templates(cwd: &Path, templates: &[JobTemplate]) -> io::Result<()> {
    let dir = cwd.join(SETTINGS_DIR);
    fs::create_dir_all(&dir)?;
    let path = dir.join(TEMPLATES_FILE);

    let mut out = String::new();
    for template in templates {
        out.push_str(&format_line(template));
        out.push('\n');
    }
    fs::write(path, out)
}

// TSV format (17 fields):
// id, name, playbook, inventory, environment,
// check, diff, become_enabled, verbosity,
// forks, timeout, limit, tags, extra_vars, extra_args,
// ssh_private_key_file, ssh_private_key_inline

fn format_line(t: &JobTemplate) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        escape(&t.id),
        escape(&t.name),
        escape(&t.playbook),
        escape(&t.inventory),
        escape_opt(&t.environment),
        bool_to_u8(t.check),
        bool_to_u8(t.diff),
        bool_to_u8(t.become_enabled),
        t.verbosity,
        t.forks.map(|v| v.to_string()).unwrap_or_default(),
        t.timeout.map(|v| v.to_string()).unwrap_or_default(),
        escape_opt(&t.limit),
        escape_opt(&t.tags),
        escape_opt(&t.extra_vars),
        escape_opt(&t.extra_args),
        escape_opt(&t.ssh_private_key_file),
        escape_opt(&t.ssh_private_key_inline),
    )
}

fn parse_line(line: &str) -> Option<JobTemplate> {
    let mut fields = line.split('\t');
    let id = unescape(fields.next()?);
    if id.is_empty() {
        return None;
    }
    let name = unescape(fields.next().unwrap_or_default());
    let playbook = unescape(fields.next().unwrap_or_default());
    let inventory = unescape(fields.next().unwrap_or_default());
    let environment = parse_opt_string(fields.next().unwrap_or_default());
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

    Some(JobTemplate {
        id,
        name,
        playbook,
        inventory,
        environment,
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
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_template_tsv_round_trip() {
        let templates = vec![
            JobTemplate {
                id: String::from("deploy-web-123456"),
                name: String::from("Deploy Web"),
                playbook: String::from("playbooks/deploy.yml"),
                inventory: String::from("inventory/prod.yml"),
                environment: Some(String::from("prod")),
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
            },
            JobTemplate {
                id: String::from("simple-000001"),
                name: String::from("Simple"),
                playbook: String::from("site.yml"),
                inventory: String::from("hosts"),
                environment: None,
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
            },
            JobTemplate {
                id: String::from("special-chars-999"),
                name: String::from("Has\ttab\nand\nnewline"),
                playbook: String::from("play\tbook.yml"),
                inventory: String::from("inv\\entory"),
                environment: Some(String::from("dev")),
                check: true,
                diff: false,
                become_enabled: false,
                verbosity: 4,
                forks: None,
                timeout: None,
                limit: None,
                tags: None,
                extra_vars: None,
                extra_args: None,
                ssh_private_key_file: None,
                ssh_private_key_inline: Some(String::from(
                    "-----BEGIN KEY-----\ndata\n-----END KEY-----",
                )),
            },
        ];

        let mut serialized = String::new();
        for t in &templates {
            serialized.push_str(&format_line(t));
            serialized.push('\n');
        }

        let mut parsed = Vec::new();
        for line in serialized.lines() {
            if let Some(t) = parse_line(line) {
                parsed.push(t);
            }
        }

        assert_eq!(parsed.len(), templates.len());
        for (original, restored) in templates.iter().zip(parsed.iter()) {
            assert_eq!(original.id, restored.id);
            assert_eq!(original.name, restored.name);
            assert_eq!(original.playbook, restored.playbook);
            assert_eq!(original.inventory, restored.inventory);
            assert_eq!(original.environment, restored.environment);
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
        }
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

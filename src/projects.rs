use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDefinition {
    pub name: String,
    pub root: PathBuf,
    pub inventory_sync_cmd: Option<String>,
    pub vars_sync_cmd: Option<String>,
    pub ssh_private_key_file: Option<String>,
    pub ssh_private_key_inline: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectRegistry {
    pub projects: Vec<ProjectDefinition>,
    pub active_idx: usize,
}

const CONFIG_DIR: &str = ".ansible-tui";
const PROJECTS_FILE: &str = "projects.tsv";

pub fn load_projects(cwd: &Path) -> io::Result<ProjectRegistry> {
    let path = projects_path(cwd);
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(ProjectRegistry {
                projects: vec![default_project(cwd)],
                active_idx: 0,
            });
        }
        Err(err) => return Err(err),
    };

    let mut projects = Vec::new();
    let mut active_idx = 0usize;
    let mut saw_active = false;

    for line in data.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() < 2 {
            continue;
        }

        let name = sanitize_loaded(fields[0]);
        let root = resolve_root(cwd, fields[1]);
        let inventory_sync_cmd = fields.get(2).and_then(|v| to_opt_text(v));
        let vars_sync_cmd = fields.get(3).and_then(|v| to_opt_text(v));
        let ssh_private_key_file = if fields.len() >= 7 {
            fields.get(4).and_then(|v| to_opt_text(v))
        } else {
            None
        };
        let ssh_private_key_inline = if fields.len() >= 7 {
            fields.get(5).and_then(|v| to_opt_multiline_text(v))
        } else {
            None
        };
        let active_field_idx = if fields.len() >= 7 { 6 } else { 4 };
        let is_active = fields
            .get(active_field_idx)
            .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let inferred_name = if name.is_empty() {
            root.file_name()
                .and_then(|v| v.to_str())
                .filter(|v| !v.trim().is_empty())
                .unwrap_or("Project")
                .to_string()
        } else {
            name
        };

        projects.push(ProjectDefinition {
            name: inferred_name,
            root,
            inventory_sync_cmd,
            vars_sync_cmd,
            ssh_private_key_file,
            ssh_private_key_inline,
        });

        if is_active && !saw_active {
            active_idx = projects.len().saturating_sub(1);
            saw_active = true;
        }
    }

    if projects.is_empty() {
        projects.push(default_project(cwd));
        active_idx = 0;
    } else if active_idx >= projects.len() {
        active_idx = 0;
    }

    Ok(ProjectRegistry {
        projects,
        active_idx,
    })
}

pub fn save_projects(cwd: &Path, registry: &ProjectRegistry) -> io::Result<()> {
    let dir = cwd.join(CONFIG_DIR);
    fs::create_dir_all(dir)?;

    let mut out = String::new();
    for (idx, project) in registry.projects.iter().enumerate() {
        let root = store_root(cwd, &project.root);
        let inventory_sync_cmd = project
            .inventory_sync_cmd
            .as_deref()
            .map(sanitize_field)
            .unwrap_or_default();
        let vars_sync_cmd = project
            .vars_sync_cmd
            .as_deref()
            .map(sanitize_field)
            .unwrap_or_default();
        let ssh_private_key_file = project
            .ssh_private_key_file
            .as_deref()
            .map(sanitize_field)
            .unwrap_or_default();
        let ssh_private_key_inline = project
            .ssh_private_key_inline
            .as_deref()
            .map(escape_multiline_field)
            .unwrap_or_default();
        let active = if idx == registry.active_idx { "1" } else { "0" };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            sanitize_field(&project.name),
            root,
            inventory_sync_cmd,
            vars_sync_cmd,
            ssh_private_key_file,
            ssh_private_key_inline,
            active
        ));
    }

    fs::write(projects_path(cwd), out)
}

pub fn default_project(cwd: &Path) -> ProjectDefinition {
    ProjectDefinition {
        name: String::from("Local"),
        root: cwd.to_path_buf(),
        inventory_sync_cmd: None,
        vars_sync_cmd: None,
        ssh_private_key_file: None,
        ssh_private_key_inline: None,
    }
}

fn projects_path(cwd: &Path) -> PathBuf {
    cwd.join(CONFIG_DIR).join(PROJECTS_FILE)
}

fn sanitize_loaded(value: &str) -> String {
    value.trim().to_string()
}

fn sanitize_field(value: &str) -> String {
    value
        .replace('\t', " ")
        .replace('\n', " ")
        .replace('\r', "")
}

fn to_opt_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn to_opt_multiline_text(value: &str) -> Option<String> {
    let unescaped = unescape_multiline_field(value);
    if unescaped.trim().is_empty() {
        None
    } else {
        Some(unescaped)
    }
}

fn escape_multiline_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape_multiline_field(raw: &str) -> String {
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

fn resolve_root(cwd: &Path, value: &str) -> PathBuf {
    let raw = value.trim();
    if raw.is_empty() {
        return cwd.to_path_buf();
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else if raw == "." {
        cwd.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn store_root(cwd: &Path, root: &Path) -> String {
    root.strip_prefix(cwd)
        .map(|p| {
            let s = p.display().to_string();
            if s.is_empty() {
                String::from(".")
            } else {
                s
            }
        })
        .unwrap_or_else(|_| root.display().to_string())
}

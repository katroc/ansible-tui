use std::path::{Path, PathBuf};
use std::process::Stdio as StdStdio;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::secrets::VaultSourceType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayPreview {
    pub name: String,
    pub host_pattern: String,
    pub tasks: Vec<TaskPreviewItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPreviewItem {
    pub name: String,
    pub tags: Vec<String>,
    pub role: Option<String>,
    pub when: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TaskPreviewRequest {
    pub cwd: PathBuf,
    pub ansible_bin: String,
    pub playbook: String,
    pub inventory: String,
    pub vault_source_type: Option<VaultSourceType>,
    pub vault_password_file: Option<String>,
    pub vault_id_label: Option<String>,
}

pub fn spawn_task_preview(req: TaskPreviewRequest, tx: UnboundedSender<Action>) {
    tokio::spawn(async move {
        let args = build_preview_args(&req);
        let mut command = Command::new(&req.ansible_bin);
        command
            .args(&args)
            .current_dir(&req.cwd)
            .stdout(StdStdio::piped())
            .stderr(StdStdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = tx.send(Action::TaskPreviewFinished {
                    success: false,
                    plays: Vec::new(),
                    message: format!("failed to start {}: {err}", req.ansible_bin),
                });
                return;
            }
        };

        let stdout_task = child.stdout.take().map(|stdout| {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut buffer = String::new();
                reader
                    .read_to_string(&mut buffer)
                    .await
                    .map(|_| buffer)
                    .map_err(|err| format!("stdout read error: {err}"))
            })
        });

        let stderr_task = child.stderr.take().map(|stderr| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            if tx.send(Action::TaskPreviewLog(strip_ansi(&line))).is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx
                                .send(Action::TaskPreviewLog(format!("stderr read error: {err}")));
                            break;
                        }
                    }
                }
            })
        });

        let status = match child.wait().await {
            Ok(status) => status,
            Err(err) => {
                let _ = tx.send(Action::TaskPreviewFinished {
                    success: false,
                    plays: Vec::new(),
                    message: format!("failed waiting for task preview process: {err}"),
                });
                return;
            }
        };

        let mut stdout = String::new();
        if let Some(task) = stdout_task {
            match task.await {
                Ok(Ok(out)) => stdout = out,
                Ok(Err(err)) => {
                    let _ = tx.send(Action::TaskPreviewLog(err));
                }
                Err(err) => {
                    let _ = tx.send(Action::TaskPreviewLog(format!(
                        "stdout task join error: {err}"
                    )));
                }
            }
        }
        if let Some(task) = stderr_task {
            let _ = task.await;
        }

        let plays = parse_list_tasks_output(&stdout);
        let total_tasks = plays.iter().map(|play| play.tasks.len()).sum::<usize>();
        let success = status.success();
        let message = if success {
            if total_tasks == 0 {
                String::from("Task preview loaded (no tasks reported)")
            } else {
                format!(
                    "Task preview loaded: {total_tasks} tasks across {} plays",
                    plays.len()
                )
            }
        } else {
            match status.code() {
                Some(code) => format!("Task preview failed (exit {code})"),
                None => String::from("Task preview failed"),
            }
        };

        let _ = tx.send(Action::TaskPreviewFinished {
            success,
            plays,
            message,
        });
    });
}

pub fn parse_list_tasks_output(stdout: &str) -> Vec<PlayPreview> {
    let mut plays = Vec::new();
    let mut current_play: Option<PlayPreview> = None;
    let mut in_tasks_section = false;

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some((host_pattern, name)) = parse_play_header(line) {
            if let Some(play) = current_play.take() {
                plays.push(play);
            }
            current_play = Some(PlayPreview {
                name,
                host_pattern,
                tasks: Vec::new(),
            });
            in_tasks_section = false;
            continue;
        }

        if line.eq_ignore_ascii_case("tasks:") {
            in_tasks_section = true;
            continue;
        }

        if !in_tasks_section {
            continue;
        }

        if let Some(task) = parse_task_line(line) {
            if let Some(play) = current_play.as_mut() {
                play.tasks.push(task);
            }
        }
    }

    if let Some(play) = current_play {
        plays.push(play);
    }

    plays
}

fn parse_play_header(line: &str) -> Option<(String, String)> {
    let remainder = line.strip_prefix("play #")?;
    let open_paren = remainder.find('(')?;
    let close_paren = remainder[open_paren + 1..].find(')')? + open_paren + 1;
    let colon = remainder[close_paren + 1..].find(':')? + close_paren + 1;

    let host_pattern = remainder[open_paren + 1..close_paren].trim().to_string();
    let name_with_suffix = remainder[colon + 1..].trim();
    let name = strip_tags_suffix(name_with_suffix);
    Some((host_pattern, name))
}

fn strip_tags_suffix(value: &str) -> String {
    value
        .split("TAGS:")
        .next()
        .unwrap_or(value)
        .trim()
        .trim_end_matches('\t')
        .trim()
        .to_string()
}

fn parse_task_line(line: &str) -> Option<TaskPreviewItem> {
    let (task_part, tags_part) = line.rsplit_once("TAGS:")?;
    let task_part = task_part.trim();
    if task_part.is_empty() {
        return None;
    }

    let (tags, when) = parse_tags_and_when(tags_part.trim());
    let (role, name) = parse_role_and_task_name(task_part);
    if name.is_empty() {
        return None;
    }

    Some(TaskPreviewItem {
        name,
        tags,
        role,
        when,
    })
}

fn parse_role_and_task_name(task_part: &str) -> (Option<String>, String) {
    if let Some((role, name)) = task_part.split_once(" : ") {
        let role = role.trim();
        let name = name.trim();
        if !role.is_empty() && !name.is_empty() {
            return (Some(role.to_string()), name.to_string());
        }
    }
    (None, task_part.trim().to_string())
}

fn parse_tags_and_when(tags_part: &str) -> (Vec<String>, Option<String>) {
    let Some(open) = tags_part.find('[') else {
        return (Vec::new(), None);
    };
    let Some(close) = tags_part.rfind(']') else {
        return (Vec::new(), None);
    };
    if close <= open {
        return (Vec::new(), None);
    }

    let content = &tags_part[open + 1..close];
    let mut tags = Vec::new();
    let mut when = None;

    for item in content
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if let Some(condition) = item.strip_prefix("when:") {
            let condition = condition.trim();
            if !condition.is_empty() {
                when = Some(condition.to_string());
            }
        } else {
            tags.push(item.to_string());
        }
    }

    (tags, when)
}

fn build_preview_args(req: &TaskPreviewRequest) -> Vec<String> {
    let mut args = vec![
        String::from("-i"),
        req.inventory.clone(),
        String::from("--list-tasks"),
        String::from("--list-tags"),
    ];
    append_vault_args(req, &mut args);
    args.push(req.playbook.clone());
    args
}

fn append_vault_args(req: &TaskPreviewRequest, args: &mut Vec<String>) {
    let Some(source_type) = req.vault_source_type else {
        return;
    };
    let vault_id_label = req
        .vault_id_label
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match source_type {
        VaultSourceType::Prompt => {
            if let Some(label) = vault_id_label {
                args.push(String::from("--vault-id"));
                args.push(format!("{label}@prompt"));
            } else {
                args.push(String::from("--ask-vault-pass"));
            }
        }
        VaultSourceType::File => {
            let Some(vault_password_file) = req
                .vault_password_file
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return;
            };
            let resolved = resolve_run_path(&req.cwd, vault_password_file);
            if let Some(label) = vault_id_label {
                args.push(String::from("--vault-id"));
                args.push(format!("{label}@{resolved}"));
            } else {
                args.push(String::from("--vault-password-file"));
                args.push(resolved);
            }
        }
    }
}

fn resolve_run_path(cwd: &Path, value: &str) -> String {
    let path = Path::new(value);
    if path.is_absolute() {
        value.to_string()
    } else {
        cwd.join(path).to_string_lossy().to_string()
    }
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && matches!(chars.peek(), Some('[')) {
            let _ = chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse_list_tasks_output;

    #[test]
    fn parse_single_play_with_tasks() {
        let output = "playbook: playbooks/site.yml\n\n  play #1 (webservers): Install nginx\tTAGS: []\n\n    tasks:\n      common : Install base packages\tTAGS: [setup]\n      nginx : Install nginx\tTAGS: [nginx, setup]\n      Install config file\tTAGS: []\n";

        let plays = parse_list_tasks_output(output);
        assert_eq!(plays.len(), 1);
        assert_eq!(plays[0].name, "Install nginx");
        assert_eq!(plays[0].host_pattern, "webservers");
        assert_eq!(plays[0].tasks.len(), 3);
    }

    #[test]
    fn parse_multi_play() {
        let output = "playbook: playbooks/site.yml\n\n  play #1 (web): Web setup\tTAGS: []\n\n    tasks:\n      Setup web\tTAGS: [web]\n\n  play #2 (db): DB setup\tTAGS: []\n\n    tasks:\n      Setup db\tTAGS: [db]\n";

        let plays = parse_list_tasks_output(output);
        assert_eq!(plays.len(), 2);
        assert_eq!(plays[0].host_pattern, "web");
        assert_eq!(plays[1].host_pattern, "db");
    }

    #[test]
    fn parse_tasks_with_roles() {
        let output = "  play #1 (all): Role test\tTAGS: []\n    tasks:\n      common : Install packages\tTAGS: [setup]\n";

        let plays = parse_list_tasks_output(output);
        assert_eq!(plays.len(), 1);
        assert_eq!(plays[0].tasks.len(), 1);
        assert_eq!(plays[0].tasks[0].role.as_deref(), Some("common"));
        assert_eq!(plays[0].tasks[0].name, "Install packages");
    }

    #[test]
    fn parse_tasks_with_tags() {
        let output = "  play #1 (all): Tags test\tTAGS: []\n    tasks:\n      Validate config\tTAGS: [lint, smoke]\n";

        let plays = parse_list_tasks_output(output);
        assert_eq!(plays.len(), 1);
        assert_eq!(plays[0].tasks[0].tags, vec!["lint", "smoke"]);
    }

    #[test]
    fn parse_empty_output() {
        let plays = parse_list_tasks_output("");
        assert!(plays.is_empty());
    }

    #[test]
    fn parse_no_tasks() {
        let output = "playbook: playbooks/site.yml\n\n  play #1 (all): No tasks\tTAGS: []\n";

        let plays = parse_list_tasks_output(output);
        assert_eq!(plays.len(), 1);
        assert!(plays[0].tasks.is_empty());
    }
}

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio as StdStdio;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

use crate::action::Action;
use crate::secrets::VaultSourceType;

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub ansible_bin: String,
    pub check: bool,
    pub diff: bool,
    pub become_enabled: bool,
    pub verbosity: u8,
    pub forks: Option<u16>,
    pub timeout: Option<u16>,
    pub limit: Option<String>,
    pub tags: Option<String>,
    pub extra_vars_files: Vec<String>,
    pub extra_vars: Option<String>,
    pub extra_args: Option<String>,
    pub ssh_private_key_file: Option<String>,
    pub ssh_private_key_inline: Option<String>,
    pub vault_source_type: Option<VaultSourceType>,
    pub vault_password_file: Option<String>,
    pub vault_id_label: Option<String>,
}

impl RunOptions {
    pub fn from_env() -> Self {
        Self {
            ansible_bin: env_var("ANSIBLE_TUI_PLAYBOOK_BIN")
                .unwrap_or_else(|| String::from("ansible-playbook")),
            check: false,
            diff: false,
            become_enabled: false,
            verbosity: env_var("ANSIBLE_TUI_VERBOSITY")
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(0)
                .min(4),
            forks: env_var("ANSIBLE_TUI_FORKS").and_then(|v| v.parse::<u16>().ok()),
            timeout: env_var("ANSIBLE_TUI_TIMEOUT").and_then(|v| v.parse::<u16>().ok()),
            limit: env_var("ANSIBLE_TUI_LIMIT"),
            tags: env_var("ANSIBLE_TUI_TAGS"),
            extra_vars_files: Vec::new(),
            extra_vars: env_var("ANSIBLE_TUI_EXTRA_VARS"),
            extra_args: env_var("ANSIBLE_TUI_EXTRA_ARGS"),
            ssh_private_key_file: None,
            ssh_private_key_inline: None,
            vault_source_type: None,
            vault_password_file: None,
            vault_id_label: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub run_id: u64,
    pub cwd: PathBuf,
    pub playbook: String,
    pub command_playbook: Option<String>,
    pub inventory: String,
    pub options: RunOptions,
    pub template_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeCandidate {
    pub label: String,
    pub ansible_bin: String,
    pub available: bool,
}

pub fn spawn_ansible_run(req: RunRequest, tx: UnboundedSender<Action>) {
    tokio::spawn(async move {
        if tx
            .send(Action::RunStarted {
                run_id: req.run_id,
                playbook: req.playbook.clone(),
                inventory: req.inventory.clone(),
                template_id: req.template_id.clone(),
            })
            .is_err()
        {
            return;
        }

        let inline_key_file = match prepare_inline_private_key_file(&req) {
            Ok(file) => file,
            Err(err) => {
                let message = format!("failed to prepare inline SSH private key: {err}");
                let _ = tx.send(Action::RunLog {
                    run_id: req.run_id,
                    line: message.clone(),
                });
                let _ = tx.send(Action::Error(message));
                let _ = tx.send(Action::RunFinished {
                    run_id: req.run_id,
                    success: false,
                    exit_code: None,
                });
                return;
            }
        };
        let args = build_args(&req, inline_key_file.as_ref().map(|file| file.path()));
        let command_line = redacted_command_line(&req.options.ansible_bin, &args);
        if tx
            .send(Action::RunLog {
                run_id: req.run_id,
                line: format!("$ {command_line}"),
            })
            .is_err()
        {
            return;
        }

        let mut command = Command::new(&req.options.ansible_bin);
        command
            .args(&args)
            .current_dir(&req.cwd)
            .stdout(StdStdio::piped())
            .stderr(StdStdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = tx.send(Action::RunLog {
                    run_id: req.run_id,
                    line: format!("failed to start {}: {err}", req.options.ansible_bin),
                });
                let _ = tx.send(Action::Error(format!(
                    "failed to start {}: {err}",
                    req.options.ansible_bin
                )));
                let _ = tx.send(Action::RunFinished {
                    run_id: req.run_id,
                    success: false,
                    exit_code: None,
                });
                return;
            }
        };

        let stdout_task = child.stdout.take().map(|stdout| {
            let tx = tx.clone();
            let run_id = req.run_id;
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx.send(Action::RunLog { run_id, line }).is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::RunLog {
                                run_id,
                                line: format!("stdout read error: {err}"),
                            });
                            break;
                        }
                    }
                }
            })
        });

        let stderr_task = child.stderr.take().map(|stderr| {
            let tx = tx.clone();
            let run_id = req.run_id;
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx
                                .send(Action::RunLog {
                                    run_id,
                                    line: format!("[stderr] {line}"),
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::RunLog {
                                run_id,
                                line: format!("stderr read error: {err}"),
                            });
                            break;
                        }
                    }
                }
            })
        });

        let status = match child.wait().await {
            Ok(status) => status,
            Err(err) => {
                let _ = tx.send(Action::RunLog {
                    run_id: req.run_id,
                    line: format!("failed waiting for process: {err}"),
                });
                let _ = tx.send(Action::RunFinished {
                    run_id: req.run_id,
                    success: false,
                    exit_code: None,
                });
                return;
            }
        };

        if let Some(task) = stdout_task {
            let _ = task.await;
        }
        if let Some(task) = stderr_task {
            let _ = task.await;
        }

        let _ = tx.send(Action::RunFinished {
            run_id: req.run_id,
            success: status.success(),
            exit_code: status.code(),
        });
    });
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn build_args(req: &RunRequest, inline_key_path: Option<&Path>) -> Vec<String> {
    let mut args = Vec::new();
    args.push(String::from("-i"));
    args.push(req.inventory.clone());

    if let Some(path) = inline_key_path {
        args.push(String::from("--private-key"));
        args.push(path.to_string_lossy().to_string());
    } else if let Some(path) = &req.options.ssh_private_key_file {
        args.push(String::from("--private-key"));
        args.push(resolve_run_path(&req.cwd, path));
    }

    if req.options.check {
        args.push(String::from("--check"));
    }
    if req.options.diff {
        args.push(String::from("--diff"));
    }
    if req.options.become_enabled {
        args.push(String::from("--become"));
    }
    if let Some(forks) = req.options.forks {
        args.push(String::from("--forks"));
        args.push(forks.to_string());
    }
    if let Some(timeout) = req.options.timeout {
        args.push(String::from("--timeout"));
        args.push(timeout.to_string());
    }
    if req.options.verbosity > 0 {
        args.push(format!("-{}", "v".repeat(req.options.verbosity as usize)));
    }
    if let Some(limit) = &req.options.limit {
        args.push(String::from("--limit"));
        args.push(limit.clone());
    }
    if let Some(tags) = &req.options.tags {
        args.push(String::from("--tags"));
        args.push(tags.clone());
    }
    for vars_file in &req.options.extra_vars_files {
        args.push(String::from("--extra-vars"));
        args.push(format!("@{}", resolve_run_path(&req.cwd, vars_file)));
    }
    if let Some(extra_vars) = &req.options.extra_vars {
        args.push(String::from("--extra-vars"));
        args.push(extra_vars.clone());
    }
    if let Some(extra_args) = &req.options.extra_args {
        args.extend(
            extra_args
                .split_whitespace()
                .map(std::string::ToString::to_string),
        );
    }
    append_vault_args(req, &mut args);

    args.push(
        req.command_playbook
            .as_ref()
            .cloned()
            .unwrap_or_else(|| req.playbook.clone()),
    );
    args
}

fn append_vault_args(req: &RunRequest, args: &mut Vec<String>) {
    let Some(source_type) = req.options.vault_source_type else {
        return;
    };
    let vault_id_label = req
        .options
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
                .options
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

struct TempKeyFile {
    path: PathBuf,
}

impl TempKeyFile {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempKeyFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn prepare_inline_private_key_file(req: &RunRequest) -> io::Result<Option<TempKeyFile>> {
    let Some(inline_key) = req.options.ssh_private_key_inline.as_ref() else {
        return Ok(None);
    };
    if inline_key.trim().is_empty() {
        return Ok(None);
    }

    let key_dir = req.cwd.join(".ansible-tui").join("keys");
    fs::create_dir_all(&key_dir)?;
    let key_path = key_dir.join(format!("run-{}.key", req.run_id));

    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(&key_path)?;
    if inline_key.ends_with('\n') {
        file.write_all(inline_key.as_bytes())?;
    } else {
        file.write_all(inline_key.as_bytes())?;
        file.write_all(b"\n")?;
    }
    file.flush()?;

    #[cfg(unix)]
    {
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(Some(TempKeyFile { path: key_path }))
}

fn resolve_run_path(cwd: &Path, raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return raw.to_string();
    }
    if let Some(suffix) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(suffix)
                .to_string_lossy()
                .to_string();
        }
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        raw.to_string()
    } else {
        cwd.join(path).to_string_lossy().to_string()
    }
}

fn redacted_command_line(ansible_bin: &str, args: &[String]) -> String {
    let mut redacted = Vec::with_capacity(args.len());
    let mut idx = 0usize;
    while idx < args.len() {
        let current = &args[idx];
        match current.as_str() {
            "--private-key" | "--vault-password-file" | "--vault-id" => {
                redacted.push(current.clone());
                if idx + 1 < args.len() {
                    redacted.push(String::from("<redacted>"));
                }
                idx += 2;
                continue;
            }
            "--extra-vars" => {
                redacted.push(current.clone());
                if idx + 1 < args.len() {
                    redacted.push(redact_extra_vars_value(&args[idx + 1]));
                }
                idx += 2;
                continue;
            }
            _ => {}
        }

        if current.starts_with("--private-key=") {
            redacted.push(String::from("--private-key=<redacted>"));
        } else if current.starts_with("--vault-password-file=") {
            redacted.push(String::from("--vault-password-file=<redacted>"));
        } else if current.starts_with("--vault-id=") {
            redacted.push(String::from("--vault-id=<redacted>"));
        } else if let Some(value) = current.strip_prefix("--extra-vars=") {
            redacted.push(format!("--extra-vars={}", redact_extra_vars_value(value)));
        } else {
            redacted.push(current.clone());
        }
        idx += 1;
    }

    if redacted.is_empty() {
        ansible_bin.to_string()
    } else {
        format!("{ansible_bin} {}", redacted.join(" "))
    }
}

fn redact_extra_vars_value(value: &str) -> String {
    if value.trim_start().starts_with('@') {
        value.to_string()
    } else {
        String::from("<redacted>")
    }
}

pub fn playbook_bin_available(playbook_bin: &str) -> bool {
    if looks_like_path(playbook_bin) {
        return Path::new(playbook_bin).is_file();
    }

    std::process::Command::new(playbook_bin)
        .arg("--version")
        .stdout(StdStdio::null())
        .stderr(StdStdio::null())
        .status()
        .is_ok()
}

pub fn discover_runtime_candidates(
    cwd: &Path,
    configured_bin: Option<&str>,
) -> Vec<RuntimeCandidate> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();

    if let Some(configured_bin) = configured_bin {
        push_candidate(
            &mut out,
            &mut seen,
            "Configured runtime",
            configured_bin.to_string(),
        );
    }

    let managed = managed_runtime_playbook(cwd);
    if managed.is_file() {
        push_candidate(
            &mut out,
            &mut seen,
            "Managed runtime (.ansible-tui/runtime)",
            managed.to_string_lossy().to_string(),
        );
    }

    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        let from_env = PathBuf::from(venv).join("bin").join("ansible-playbook");
        if from_env.is_file() {
            push_candidate(
                &mut out,
                &mut seen,
                "Current VIRTUAL_ENV",
                from_env.to_string_lossy().to_string(),
            );
        }
    }

    for rel in [".venv/bin/ansible-playbook", "venv/bin/ansible-playbook"] {
        let candidate = cwd.join(rel);
        if candidate.is_file() {
            push_candidate(
                &mut out,
                &mut seen,
                &format!("Project venv ({rel})"),
                candidate.to_string_lossy().to_string(),
            );
        }
    }

    if playbook_bin_available("ansible-playbook") {
        push_candidate(
            &mut out,
            &mut seen,
            "System PATH (ansible-playbook)",
            String::from("ansible-playbook"),
        );
    }

    for path_bin in discover_from_path() {
        push_candidate(&mut out, &mut seen, "PATH candidate", path_bin);
    }

    out
}

pub fn managed_runtime_playbook(cwd: &Path) -> PathBuf {
    cwd.join(".ansible-tui")
        .join("runtime")
        .join("bin")
        .join("ansible-playbook")
}

pub fn spawn_bootstrap_managed_runtime(cwd: PathBuf, tx: UnboundedSender<Action>) {
    tokio::spawn(async move {
        let python_bin =
            env_var("ANSIBLE_TUI_PYTHON_BIN").unwrap_or_else(|| String::from("python3"));
        let runtime_dir = cwd.join(".ansible-tui").join("runtime");
        let runtime_python = runtime_dir.join("bin").join("python");
        let runtime_playbook = managed_runtime_playbook(&cwd);

        if tx
            .send(Action::RuntimeBootstrapLog(format!(
                "Using python runtime: {python_bin}"
            )))
            .is_err()
        {
            return;
        }

        if !run_logged_command(
            &tx,
            &python_bin,
            &[
                String::from("-m"),
                String::from("venv"),
                runtime_dir.to_string_lossy().to_string(),
            ],
            "Creating virtual environment",
        )
        .await
        {
            let _ = tx.send(Action::RuntimeBootstrapFinished {
                success: false,
                ansible_bin: None,
                message: String::from("Failed creating virtual environment"),
            });
            return;
        }

        if !run_logged_command(
            &tx,
            &runtime_python.to_string_lossy(),
            &[
                String::from("-m"),
                String::from("pip"),
                String::from("install"),
                String::from("--upgrade"),
                String::from("pip"),
            ],
            "Upgrading pip",
        )
        .await
        {
            let _ = tx.send(Action::RuntimeBootstrapFinished {
                success: false,
                ansible_bin: None,
                message: String::from("Failed upgrading pip"),
            });
            return;
        }

        if !run_logged_command(
            &tx,
            &runtime_python.to_string_lossy(),
            &[
                String::from("-m"),
                String::from("pip"),
                String::from("install"),
                String::from("ansible-core==2.17.*"),
            ],
            "Installing ansible-core",
        )
        .await
        {
            let _ = tx.send(Action::RuntimeBootstrapFinished {
                success: false,
                ansible_bin: None,
                message: String::from("Failed installing ansible-core"),
            });
            return;
        }

        if !runtime_playbook.is_file() {
            let _ = tx.send(Action::RuntimeBootstrapFinished {
                success: false,
                ansible_bin: None,
                message: format!(
                    "Bootstrap finished but ansible-playbook missing at {}",
                    runtime_playbook.display()
                ),
            });
            return;
        }

        let _ = tx.send(Action::RuntimeBootstrapFinished {
            success: true,
            ansible_bin: Some(runtime_playbook.to_string_lossy().to_string()),
            message: String::from("Managed runtime ready"),
        });
    });
}

pub fn spawn_project_sync(
    cwd: PathBuf,
    command_line: String,
    label: String,
    tx: UnboundedSender<Action>,
) {
    tokio::spawn(async move {
        if tx
            .send(Action::ProjectSyncLog(format!("$ {}", command_line)))
            .is_err()
        {
            return;
        }

        let mut command = Command::new("sh");
        command
            .arg("-lc")
            .arg(&command_line)
            .current_dir(&cwd)
            .stdout(StdStdio::piped())
            .stderr(StdStdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let message = format!("Failed to start {label} sync: {err}");
                let _ = tx.send(Action::ProjectSyncLog(message.clone()));
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message,
                });
                return;
            }
        };

        let stdout_task = child.stdout.take().map(|stdout| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx.send(Action::ProjectSyncLog(line)).is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::ProjectSyncLog(format!(
                                "sync stdout read error: {err}"
                            )));
                            break;
                        }
                    }
                }
            })
        });

        let stderr_task = child.stderr.take().map(|stderr| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx
                                .send(Action::ProjectSyncLog(format!("[stderr] {line}")))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::ProjectSyncLog(format!(
                                "sync stderr read error: {err}"
                            )));
                            break;
                        }
                    }
                }
            })
        });

        let status = match child.wait().await {
            Ok(status) => status,
            Err(err) => {
                let message = format!("Failed waiting for {label} sync: {err}");
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message,
                });
                return;
            }
        };

        if let Some(task) = stdout_task {
            let _ = task.await;
        }
        if let Some(task) = stderr_task {
            let _ = task.await;
        }

        let message = match status.code() {
            Some(code) => {
                if status.success() {
                    format!("{label} sync completed (exit {code})")
                } else {
                    format!("{label} sync failed (exit {code})")
                }
            }
            None => {
                if status.success() {
                    format!("{label} sync completed")
                } else {
                    format!("{label} sync failed")
                }
            }
        };
        let _ = tx.send(Action::ProjectSyncFinished {
            success: status.success(),
            message,
        });
    });
}

pub fn ansible_vault_bin_for_playbook(playbook_bin: &str) -> String {
    if looks_like_path(playbook_bin) {
        let path = PathBuf::from(playbook_bin);
        let name = path.file_name().and_then(|value| value.to_str());
        if name == Some("ansible-playbook") {
            return path
                .with_file_name("ansible-vault")
                .to_string_lossy()
                .to_string();
        }
    } else if playbook_bin == "ansible-playbook" {
        return String::from("ansible-vault");
    } else if let Some(prefix) = playbook_bin.strip_suffix("ansible-playbook") {
        return format!("{prefix}ansible-vault");
    }

    String::from("ansible-vault")
}

fn build_vault_auth_args(
    cwd: &Path,
    vault_source_type: VaultSourceType,
    vault_password_file: Option<String>,
    vault_id_label: Option<String>,
) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let vault_id_label = vault_id_label
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string);
    match vault_source_type {
        VaultSourceType::Prompt => {
            if let Some(label) = vault_id_label {
                args.push(String::from("--vault-id"));
                args.push(format!("{label}@prompt"));
            } else {
                args.push(String::from("--ask-vault-pass"));
            }
        }
        VaultSourceType::File => {
            let Some(vault_password_file) = vault_password_file
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return Err(String::from(
                    "Vault source is file but no vault password file was provided",
                ));
            };
            let resolved_password = resolve_run_path(cwd, vault_password_file);
            if let Some(label) = vault_id_label {
                args.push(String::from("--vault-id"));
                args.push(format!("{label}@{resolved_password}"));
            } else {
                args.push(String::from("--vault-password-file"));
                args.push(resolved_password);
            }
        }
    }

    Ok(args)
}

pub fn spawn_ansible_vault_create_file(
    cwd: PathBuf,
    playbook_bin: String,
    target_file: String,
    content: String,
    vault_source_type: VaultSourceType,
    vault_password_file: Option<String>,
    vault_id_label: Option<String>,
    tx: UnboundedSender<Action>,
) {
    tokio::spawn(async move {
        let vault_bin = ansible_vault_bin_for_playbook(&playbook_bin);
        let resolved_target = resolve_run_path(&cwd, &target_file);
        let target_path = PathBuf::from(&resolved_target);
        if target_path.exists() {
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message: format!("Vault file already exists: {}", target_path.display()),
            });
            return;
        }
        if let Some(parent) = target_path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message: format!(
                        "Failed to create vault directory {}: {err}",
                        parent.display()
                    ),
                });
                return;
            }
        }

        let tmp_dir = cwd.join(".ansible-tui").join("vault");
        if let Err(err) = fs::create_dir_all(&tmp_dir) {
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message: format!("Failed to prepare vault temp directory: {err}"),
            });
            return;
        }
        let tmp_stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let tmp_path = tmp_dir.join(format!("vault-create-{tmp_stamp}.yml"));

        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut tmp_file = match options.open(&tmp_path) {
            Ok(file) => file,
            Err(err) => {
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message: format!(
                        "Failed to create temporary vault file {}: {err}",
                        tmp_path.display()
                    ),
                });
                return;
            }
        };
        let normalized_content = content.replace("\r\n", "\n").replace('\r', "\n");
        let write_result = if normalized_content.ends_with('\n') {
            tmp_file.write_all(normalized_content.as_bytes())
        } else {
            tmp_file
                .write_all(normalized_content.as_bytes())
                .and_then(|_| tmp_file.write_all(b"\n"))
        };
        if let Err(err) = write_result.and_then(|_| tmp_file.flush()) {
            let _ = fs::remove_file(&tmp_path);
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message: format!("Failed to write temporary vault file: {err}"),
            });
            return;
        }
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
        }

        let mut args = vec![String::from("encrypt")];
        let auth_args = match build_vault_auth_args(
            &cwd,
            vault_source_type,
            vault_password_file,
            vault_id_label,
        ) {
            Ok(value) => value,
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message: err,
                });
                return;
            }
        };
        args.extend(auth_args);
        args.push(tmp_path.to_string_lossy().to_string());

        if tx
            .send(Action::ProjectSyncLog(format!(
                "$ {}",
                redacted_command_line(&vault_bin, &args)
            )))
            .is_err()
        {
            let _ = fs::remove_file(&tmp_path);
            return;
        }

        let mut command = Command::new(&vault_bin);
        command
            .args(&args)
            .current_dir(&cwd)
            .stdout(StdStdio::piped())
            .stderr(StdStdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                let message = format!("Failed to start {}: {err}", vault_bin);
                let _ = tx.send(Action::ProjectSyncLog(message.clone()));
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message,
                });
                return;
            }
        };

        let stdout_task = child.stdout.take().map(|stdout| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx.send(Action::ProjectSyncLog(line)).is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::ProjectSyncLog(format!(
                                "vault stdout read error: {err}"
                            )));
                            break;
                        }
                    }
                }
            })
        });

        let stderr_task = child.stderr.take().map(|stderr| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx
                                .send(Action::ProjectSyncLog(format!("[stderr] {line}")))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::ProjectSyncLog(format!(
                                "vault stderr read error: {err}"
                            )));
                            break;
                        }
                    }
                }
            })
        });

        let status = match child.wait().await {
            Ok(status) => status,
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                let message = format!("Failed waiting for vault encryption: {err}");
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message,
                });
                return;
            }
        };

        if let Some(task) = stdout_task {
            let _ = task.await;
        }
        if let Some(task) = stderr_task {
            let _ = task.await;
        }

        if !status.success() {
            let _ = fs::remove_file(&tmp_path);
            let message = match status.code() {
                Some(code) => format!("Vault encryption failed (exit {code})"),
                None => String::from("Vault encryption failed"),
            };
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message,
            });
            return;
        }

        let move_result = fs::rename(&tmp_path, &target_path).or_else(|_| {
            fs::copy(&tmp_path, &target_path)
                .map(|_| ())
                .and_then(|_| fs::remove_file(&tmp_path))
        });
        if let Err(err) = move_result {
            let _ = fs::remove_file(&tmp_path);
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message: format!(
                    "Vault encrypted, but failed moving file to {}: {err}",
                    target_path.display()
                ),
            });
            return;
        }

        let _ = tx.send(Action::ProjectSyncFinished {
            success: true,
            message: format!("Vault file created: {}", target_path.display()),
        });
    });
}

pub fn spawn_ansible_vault_update_file(
    cwd: PathBuf,
    playbook_bin: String,
    target_file: String,
    content: String,
    vault_source_type: VaultSourceType,
    vault_password_file: Option<String>,
    vault_id_label: Option<String>,
    tx: UnboundedSender<Action>,
) {
    tokio::spawn(async move {
        let vault_bin = ansible_vault_bin_for_playbook(&playbook_bin);
        let resolved_target = resolve_run_path(&cwd, &target_file);
        let target_path = PathBuf::from(&resolved_target);
        if !target_path.is_file() {
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message: format!("Vault file does not exist: {}", target_path.display()),
            });
            return;
        }

        let tmp_dir = cwd.join(".ansible-tui").join("vault");
        if let Err(err) = fs::create_dir_all(&tmp_dir) {
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message: format!("Failed to prepare vault temp directory: {err}"),
            });
            return;
        }
        let tmp_stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let tmp_path = tmp_dir.join(format!("vault-update-{tmp_stamp}.yml"));

        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut tmp_file = match options.open(&tmp_path) {
            Ok(file) => file,
            Err(err) => {
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message: format!(
                        "Failed to create temporary vault file {}: {err}",
                        tmp_path.display()
                    ),
                });
                return;
            }
        };
        let normalized_content = content.replace("\r\n", "\n").replace('\r', "\n");
        let write_result = if normalized_content.ends_with('\n') {
            tmp_file.write_all(normalized_content.as_bytes())
        } else {
            tmp_file
                .write_all(normalized_content.as_bytes())
                .and_then(|_| tmp_file.write_all(b"\n"))
        };
        if let Err(err) = write_result.and_then(|_| tmp_file.flush()) {
            let _ = fs::remove_file(&tmp_path);
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message: format!("Failed to write temporary vault file: {err}"),
            });
            return;
        }
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600));
        }

        let mut args = vec![String::from("encrypt")];
        let auth_args = match build_vault_auth_args(
            &cwd,
            vault_source_type,
            vault_password_file,
            vault_id_label,
        ) {
            Ok(value) => value,
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message: err,
                });
                return;
            }
        };
        args.extend(auth_args);
        args.push(tmp_path.to_string_lossy().to_string());

        if tx
            .send(Action::ProjectSyncLog(format!(
                "$ {}",
                redacted_command_line(&vault_bin, &args)
            )))
            .is_err()
        {
            let _ = fs::remove_file(&tmp_path);
            return;
        }

        let mut command = Command::new(&vault_bin);
        command
            .args(&args)
            .current_dir(&cwd)
            .stdout(StdStdio::piped())
            .stderr(StdStdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                let message = format!("Failed to start {}: {err}", vault_bin);
                let _ = tx.send(Action::ProjectSyncLog(message.clone()));
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message,
                });
                return;
            }
        };

        let stdout_task = child.stdout.take().map(|stdout| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx.send(Action::ProjectSyncLog(line)).is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::ProjectSyncLog(format!(
                                "vault stdout read error: {err}"
                            )));
                            break;
                        }
                    }
                }
            })
        });

        let stderr_task = child.stderr.take().map(|stderr| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx
                                .send(Action::ProjectSyncLog(format!("[stderr] {line}")))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::ProjectSyncLog(format!(
                                "vault stderr read error: {err}"
                            )));
                            break;
                        }
                    }
                }
            })
        });

        let status = match child.wait().await {
            Ok(status) => status,
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                let message = format!("Failed waiting for vault encryption: {err}");
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message,
                });
                return;
            }
        };

        if let Some(task) = stdout_task {
            let _ = task.await;
        }
        if let Some(task) = stderr_task {
            let _ = task.await;
        }

        if !status.success() {
            let _ = fs::remove_file(&tmp_path);
            let message = match status.code() {
                Some(code) => format!("Vault encryption failed (exit {code})"),
                None => String::from("Vault encryption failed"),
            };
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message,
            });
            return;
        }

        let move_result = fs::rename(&tmp_path, &target_path).or_else(|_| {
            fs::copy(&tmp_path, &target_path)
                .map(|_| ())
                .and_then(|_| fs::remove_file(&tmp_path))
        });
        if let Err(err) = move_result {
            let _ = fs::remove_file(&tmp_path);
            let _ = tx.send(Action::ProjectSyncFinished {
                success: false,
                message: format!(
                    "Vault encrypted, but failed moving file to {}: {err}",
                    target_path.display()
                ),
            });
            return;
        }

        let _ = tx.send(Action::ProjectSyncFinished {
            success: true,
            message: format!("Vault file updated: {}", target_path.display()),
        });
    });
}

pub fn spawn_ansible_vault_view_file(
    cwd: PathBuf,
    playbook_bin: String,
    target_file: String,
    vault_source_type: VaultSourceType,
    vault_password_file: Option<String>,
    vault_id_label: Option<String>,
    tx: UnboundedSender<Action>,
) {
    tokio::spawn(async move {
        let vault_bin = ansible_vault_bin_for_playbook(&playbook_bin);
        let resolved_target = resolve_run_path(&cwd, &target_file);
        let target_path = PathBuf::from(&resolved_target);
        if !target_path.is_file() {
            let _ = tx.send(Action::VaultEditLoaded {
                success: false,
                path: target_path.to_string_lossy().to_string(),
                content: None,
                message: format!("Vault file does not exist: {}", target_path.display()),
            });
            return;
        }

        let mut args = vec![String::from("view")];
        let auth_args = match build_vault_auth_args(
            &cwd,
            vault_source_type,
            vault_password_file,
            vault_id_label,
        ) {
            Ok(value) => value,
            Err(err) => {
                let _ = tx.send(Action::VaultEditLoaded {
                    success: false,
                    path: target_path.to_string_lossy().to_string(),
                    content: None,
                    message: err,
                });
                return;
            }
        };
        args.extend(auth_args);
        args.push(target_path.to_string_lossy().to_string());

        if tx
            .send(Action::ProjectSyncLog(format!(
                "$ {}",
                redacted_command_line(&vault_bin, &args)
            )))
            .is_err()
        {
            return;
        }

        let mut command = Command::new(&vault_bin);
        command
            .args(&args)
            .current_dir(&cwd)
            .stdout(StdStdio::piped())
            .stderr(StdStdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = tx.send(Action::VaultEditLoaded {
                    success: false,
                    path: target_path.to_string_lossy().to_string(),
                    content: None,
                    message: format!("Failed to start {}: {err}", vault_bin),
                });
                return;
            }
        };

        let stdout_task = child.stdout.take().map(|mut stdout| {
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let _ = stdout.read_to_end(&mut buffer).await;
                buffer
            })
        });
        let stderr_task = child.stderr.take().map(|mut stderr| {
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                let _ = stderr.read_to_end(&mut buffer).await;
                buffer
            })
        });

        let status = match child.wait().await {
            Ok(status) => status,
            Err(err) => {
                let _ = tx.send(Action::VaultEditLoaded {
                    success: false,
                    path: target_path.to_string_lossy().to_string(),
                    content: None,
                    message: format!("Failed waiting for vault view: {err}"),
                });
                return;
            }
        };

        let stdout_bytes = if let Some(task) = stdout_task {
            task.await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let stderr_bytes = if let Some(task) = stderr_task {
            task.await.unwrap_or_default()
        } else {
            Vec::new()
        };

        if !status.success() {
            let stderr_text = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
            if !stderr_text.is_empty() {
                let _ = tx.send(Action::ProjectSyncLog(format!("[stderr] {stderr_text}")));
            }
            let message = match status.code() {
                Some(code) => format!("Vault view failed (exit {code})"),
                None => String::from("Vault view failed"),
            };
            let _ = tx.send(Action::VaultEditLoaded {
                success: false,
                path: target_path.to_string_lossy().to_string(),
                content: None,
                message,
            });
            return;
        }

        let content = String::from_utf8_lossy(&stdout_bytes).replace("\r\n", "\n");
        let _ = tx.send(Action::VaultEditLoaded {
            success: true,
            path: target_path.to_string_lossy().to_string(),
            content: Some(content),
            message: format!("Vault decrypted for edit: {}", target_path.display()),
        });
    });
}

pub fn spawn_git_clone(git_url: String, destination: PathBuf, tx: UnboundedSender<Action>) {
    tokio::spawn(async move {
        let command_line = format!("git clone {} {}", git_url, destination.display());
        if tx
            .send(Action::ProjectSyncLog(format!("$ {}", command_line)))
            .is_err()
        {
            return;
        }

        let mut command = Command::new("git");
        command
            .arg("clone")
            .arg(&git_url)
            .arg(&destination)
            .stdout(StdStdio::piped())
            .stderr(StdStdio::piped());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let message = format!("Failed to start git clone: {err}");
                let _ = tx.send(Action::ProjectSyncLog(message.clone()));
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message,
                });
                return;
            }
        };

        let stdout_task = child.stdout.take().map(|stdout| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx.send(Action::ProjectSyncLog(line)).is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::ProjectSyncLog(format!(
                                "git clone stdout read error: {err}"
                            )));
                            break;
                        }
                    }
                }
            })
        });

        let stderr_task = child.stderr.take().map(|stderr| {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            let line = strip_ansi(&line);
                            if tx
                                .send(Action::ProjectSyncLog(format!("[stderr] {line}")))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            let _ = tx.send(Action::ProjectSyncLog(format!(
                                "git clone stderr read error: {err}"
                            )));
                            break;
                        }
                    }
                }
            })
        });

        let status = match child.wait().await {
            Ok(status) => status,
            Err(err) => {
                let message = format!("Failed waiting for git clone: {err}");
                let _ = tx.send(Action::ProjectSyncFinished {
                    success: false,
                    message,
                });
                return;
            }
        };

        if let Some(task) = stdout_task {
            let _ = task.await;
        }
        if let Some(task) = stderr_task {
            let _ = task.await;
        }

        let message = match status.code() {
            Some(code) => {
                if status.success() {
                    format!("git clone completed (exit {code})")
                } else {
                    format!("git clone failed (exit {code})")
                }
            }
            None => {
                if status.success() {
                    String::from("git clone completed")
                } else {
                    String::from("git clone failed")
                }
            }
        };
        let _ = tx.send(Action::ProjectSyncFinished {
            success: status.success(),
            message,
        });
    });
}

fn push_candidate(
    out: &mut Vec<RuntimeCandidate>,
    seen: &mut BTreeSet<String>,
    label: &str,
    ansible_bin: String,
) {
    let available = playbook_bin_available(&ansible_bin);
    let effective_label = if available {
        label.to_string()
    } else {
        format!("{label} (unavailable)")
    };
    if seen.insert(ansible_bin.clone()) {
        out.push(RuntimeCandidate {
            label: effective_label,
            ansible_bin,
            available,
        });
    }
}

async fn run_logged_command(
    tx: &UnboundedSender<Action>,
    bin: &str,
    args: &[String],
    step: &str,
) -> bool {
    if tx
        .send(Action::RuntimeBootstrapLog(format!(
            "{step}: {} {}",
            bin,
            args.join(" ")
        )))
        .is_err()
    {
        return false;
    }

    let output = Command::new(bin)
        .args(args)
        .stdout(StdStdio::piped())
        .stderr(StdStdio::piped())
        .output()
        .await;

    let output = match output {
        Ok(output) => output,
        Err(err) => {
            let _ = tx.send(Action::RuntimeBootstrapLog(format!(
                "{step} failed to start: {err}"
            )));
            return false;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().take(40) {
        let _ = tx.send(Action::RuntimeBootstrapLog(line.to_string()));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines().take(40) {
        let _ = tx.send(Action::RuntimeBootstrapLog(format!("[stderr] {line}")));
    }

    if output.status.success() {
        let _ = tx.send(Action::RuntimeBootstrapLog(format!("{step}: complete")));
        true
    } else {
        let _ = tx.send(Action::RuntimeBootstrapLog(format!(
            "{step}: failed with status {:?}",
            output.status.code()
        )));
        false
    }
}

fn discover_from_path() -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();

    let Some(path) = std::env::var_os("PATH") else {
        return out;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("ansible-playbook");
        if candidate.is_file() {
            let candidate = candidate.to_string_lossy().to_string();
            if seen.insert(candidate.clone()) {
                out.push(candidate);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_request_with_options(options: RunOptions) -> RunRequest {
        RunRequest {
            run_id: 1,
            cwd: PathBuf::from("/tmp/project"),
            playbook: String::from("playbooks/site.yml"),
            command_playbook: None,
            inventory: String::from("inventory/hosts.yml"),
            options,
            template_id: None,
        }
    }

    #[test]
    fn build_args_supports_vault_prompt_with_id() {
        let req = run_request_with_options(RunOptions {
            ansible_bin: String::from("ansible-playbook"),
            vault_source_type: Some(VaultSourceType::Prompt),
            vault_id_label: Some(String::from("prod")),
            ..RunOptions::default()
        });

        let args = build_args(&req, None);
        assert!(args.contains(&String::from("--vault-id")));
        assert!(args.contains(&String::from("prod@prompt")));
    }

    #[test]
    fn build_args_supports_vault_file_without_id() {
        let req = run_request_with_options(RunOptions {
            ansible_bin: String::from("ansible-playbook"),
            vault_source_type: Some(VaultSourceType::File),
            vault_password_file: Some(String::from(".secrets/vault.pass")),
            ..RunOptions::default()
        });

        let args = build_args(&req, None);
        assert!(args.contains(&String::from("--vault-password-file")));
        assert!(
            args.contains(&String::from("/tmp/project/.secrets/vault.pass")),
            "expected resolved vault password file path"
        );
    }

    #[test]
    fn redacted_command_masks_sensitive_args() {
        let args = vec![
            String::from("-i"),
            String::from("inventory/hosts.yml"),
            String::from("--private-key"),
            String::from("/tmp/key.pem"),
            String::from("--extra-vars"),
            String::from("{\"password\":\"secret\"}"),
            String::from("--vault-password-file"),
            String::from("/tmp/vault.pass"),
            String::from("--vault-id=prod@/tmp/vault.pass"),
            String::from("playbooks/site.yml"),
        ];
        let command = redacted_command_line("ansible-playbook", &args);
        assert!(command.contains("--private-key <redacted>"));
        assert!(command.contains("--extra-vars <redacted>"));
        assert!(command.contains("--vault-password-file <redacted>"));
        assert!(command.contains("--vault-id=<redacted>"));
        assert!(!command.contains("/tmp/key.pem"));
        assert!(!command.contains("/tmp/vault.pass"));
        assert!(!command.contains("secret"));
    }

    #[test]
    fn ansible_vault_bin_resolution_tracks_playbook_binary() {
        assert_eq!(
            ansible_vault_bin_for_playbook("ansible-playbook"),
            "ansible-vault"
        );
        assert_eq!(
            ansible_vault_bin_for_playbook("/opt/runtime/bin/ansible-playbook"),
            "/opt/runtime/bin/ansible-vault"
        );
    }
}

fn looks_like_path(value: &str) -> bool {
    value.contains('/') || value.starts_with('.')
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

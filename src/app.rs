use std::cmp::{max, min};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use crossterm::cursor::MoveTo;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use tokio::sync::mpsc::UnboundedSender;
use walkdir::WalkDir;

use crate::action::Action;
use crate::ansible_cfg::{
    load_ansible_cfg_settings, save_ansible_cfg_settings, AnsibleCfgSettings,
};
use crate::config::{load_app_config, save_app_config, AppConfig};
use crate::input::set_input_paused;
use crate::playbook_settings::{
    cycle_u16, load_playbook_settings, save_playbook_settings, PlaybookSettings,
};
use crate::run::{
    discover_runtime_candidates, playbook_bin_available, spawn_ansible_run,
    spawn_bootstrap_managed_runtime, RunOptions, RunRequest, RuntimeCandidate,
};
use crate::run_store::{load_runs, save_run};

const MAX_LOG_LINES: usize = 1_000;
const MAX_RUNTIME_LOG_LINES: usize = 120;
const AUTO_DISCOVERY_INTERVAL: Duration = Duration::from_secs(2);
const PLAYBOOK_SETTINGS_FIELD_COUNT: usize = 10;
const PLAYBOOK_SETTINGS_TEXT_FIELD_START: usize = 6;
const GLOBAL_SETTINGS_FIELD_COUNT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Inventory,
    Playbooks,
    Settings,
}

impl View {
    pub fn all() -> [View; 4] {
        [
            View::Dashboard,
            View::Inventory,
            View::Playbooks,
            View::Settings,
        ]
    }

    pub fn title(self) -> &'static str {
        match self {
            View::Dashboard => "Dashboard",
            View::Inventory => "Inventory",
            View::Playbooks => "Playbooks",
            View::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Succeeded,
    Failed,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Succeeded => "succeeded",
            RunStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryWizardFocus {
    Tree,
    Groups,
    Hosts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InventoryWizardInputMode {
    Filename,
    AddHost,
    AddGroup,
}

#[derive(Debug, Clone)]
pub struct RunRecord {
    pub id: u64,
    pub playbook: String,
    pub inventory: String,
    pub status: RunStatus,
    pub started_at: DateTime<Local>,
    pub finished_at: Option<DateTime<Local>>,
    pub exit_code: Option<i32>,
    pub logs: Vec<String>,
}

pub struct App {
    pub cwd: PathBuf,
    pub inventories: Vec<PathBuf>,
    pub playbooks: Vec<PathBuf>,
    pub runs: Vec<RunRecord>,
    pub playbook_settings: BTreeMap<String, PlaybookSettings>,
    selected_run_by_playbook: BTreeMap<String, u64>,
    selected_inventory_by_playbook: BTreeMap<String, String>,
    pub ansible_cfg: AnsibleCfgSettings,
    pub run_options: RunOptions,
    pub status_line: String,
    pub view_idx: usize,
    pub inventory_idx: usize,
    pub playbook_idx: usize,
    pub playbooks_focus_runs: bool,
    pub run_idx: usize,
    pub log_select_mode: bool,
    pub log_cursor: usize,
    pub log_anchor: Option<usize>,
    pub inventory_create_open: bool,
    pub inventory_create_buffer: String,
    pub inventory_editor_open: bool,
    pub inventory_editor_buffer: String,
    pub inventory_editor_path: Option<PathBuf>,
    pub inventory_editor_dirty: bool,
    pub inventory_edit_mode_open: bool,
    pub inventory_edit_mode_idx: usize,
    pub inventory_wizard_open: bool,
    pub inventory_wizard_edit_path: Option<PathBuf>,
    pub inventory_wizard_filename: String,
    pub inventory_wizard_hosts: Vec<String>,
    pub inventory_wizard_groups: Vec<String>,
    pub inventory_wizard_assignments: BTreeMap<String, Vec<String>>,
    pub inventory_wizard_group_children: BTreeMap<String, Vec<String>>,
    pub inventory_wizard_focus: InventoryWizardFocus,
    pub inventory_wizard_target_group: Option<String>,
    pub inventory_wizard_tree_idx: usize,
    pub inventory_wizard_host_idx: usize,
    pub inventory_wizard_group_idx: usize,
    inventory_wizard_input_mode: Option<InventoryWizardInputMode>,
    pub inventory_wizard_input_buffer: String,
    pub runtime_prompt_open: bool,
    pub runtime_candidates: Vec<RuntimeCandidate>,
    pub runtime_candidate_idx: usize,
    pub runtime_bootstrapping: bool,
    pub runtime_logs: Vec<String>,
    pub settings_editor_open: bool,
    pub settings_editor_field_idx: usize,
    pub settings_editor_text_mode: bool,
    pub settings_editor_text_buffer: String,
    pub global_settings_field_idx: usize,
    pub global_settings_text_mode: bool,
    pub global_settings_text_buffer: String,
    pub should_quit: bool,
    needs_full_redraw: bool,
    pending_inventory_delete: Option<PathBuf>,
    last_auto_discovery_at: Instant,
    next_run_id: u64,
}

impl App {
    pub fn new(cwd: PathBuf) -> Self {
        let mut run_options = RunOptions::from_env();
        let mut status_line = String::from("Ready. Press r to run selected playbook.");
        match load_app_config(&cwd) {
            Ok(config) => Self::apply_loaded_global_config(&mut run_options, config),
            Err(err) => {
                status_line = format!("config load failed: {err}");
            }
        }

        let mut app = Self {
            cwd,
            inventories: Vec::new(),
            playbooks: Vec::new(),
            runs: Vec::new(),
            playbook_settings: BTreeMap::new(),
            selected_run_by_playbook: BTreeMap::new(),
            selected_inventory_by_playbook: BTreeMap::new(),
            ansible_cfg: AnsibleCfgSettings::default(),
            run_options,
            status_line,
            view_idx: 0,
            inventory_idx: 0,
            playbook_idx: 0,
            playbooks_focus_runs: false,
            run_idx: 0,
            log_select_mode: false,
            log_cursor: 0,
            log_anchor: None,
            inventory_create_open: false,
            inventory_create_buffer: String::new(),
            inventory_editor_open: false,
            inventory_editor_buffer: String::new(),
            inventory_editor_path: None,
            inventory_editor_dirty: false,
            inventory_edit_mode_open: false,
            inventory_edit_mode_idx: 0,
            inventory_wizard_open: false,
            inventory_wizard_edit_path: None,
            inventory_wizard_filename: String::new(),
            inventory_wizard_hosts: Vec::new(),
            inventory_wizard_groups: Vec::new(),
            inventory_wizard_assignments: BTreeMap::new(),
            inventory_wizard_group_children: BTreeMap::new(),
            inventory_wizard_focus: InventoryWizardFocus::Tree,
            inventory_wizard_target_group: None,
            inventory_wizard_tree_idx: 0,
            inventory_wizard_host_idx: 0,
            inventory_wizard_group_idx: 0,
            inventory_wizard_input_mode: None,
            inventory_wizard_input_buffer: String::new(),
            runtime_prompt_open: false,
            runtime_candidates: Vec::new(),
            runtime_candidate_idx: 0,
            runtime_bootstrapping: false,
            runtime_logs: Vec::new(),
            settings_editor_open: false,
            settings_editor_field_idx: 0,
            settings_editor_text_mode: false,
            settings_editor_text_buffer: String::new(),
            global_settings_field_idx: 0,
            global_settings_text_mode: false,
            global_settings_text_buffer: String::new(),
            should_quit: false,
            needs_full_redraw: false,
            pending_inventory_delete: None,
            last_auto_discovery_at: Instant::now(),
            next_run_id: 1,
        };
        app.refresh_project();
        app.restore_playbook_settings();
        app.restore_ansible_cfg_settings();
        app.restore_history();
        app.refresh_runtime_candidates();
        app.warn_if_playbook_bin_missing();
        app
    }

    pub fn current_view(&self) -> View {
        View::all()[self.view_idx]
    }

    pub fn take_full_redraw_request(&mut self) -> bool {
        let requested = self.needs_full_redraw;
        self.needs_full_redraw = false;
        requested
    }

    fn apply_loaded_global_config(run_options: &mut RunOptions, config: AppConfig) {
        if std::env::var("ANSIBLE_TUI_PLAYBOOK_BIN").is_err() {
            if let Some(bin) = config.ansible_bin {
                run_options.ansible_bin = bin;
            }
        }
        if let Some(check) = config.check {
            run_options.check = check;
        }
        if let Some(diff) = config.diff {
            run_options.diff = diff;
        }
        if let Some(become_enabled) = config.become_enabled {
            run_options.become_enabled = become_enabled;
        }
        if std::env::var("ANSIBLE_TUI_VERBOSITY").is_err() {
            if let Some(verbosity) = config.verbosity {
                run_options.verbosity = verbosity.min(4);
            }
        }
        if std::env::var("ANSIBLE_TUI_FORKS").is_err() {
            run_options.forks = config.forks;
        }
        if std::env::var("ANSIBLE_TUI_TIMEOUT").is_err() {
            run_options.timeout = config.timeout;
        }
        if std::env::var("ANSIBLE_TUI_LIMIT").is_err() {
            run_options.limit = config.limit;
        }
        if std::env::var("ANSIBLE_TUI_TAGS").is_err() {
            run_options.tags = config.tags;
        }
        if std::env::var("ANSIBLE_TUI_EXTRA_VARS").is_err() {
            run_options.extra_vars = config.extra_vars;
        }
        if std::env::var("ANSIBLE_TUI_EXTRA_ARGS").is_err() {
            run_options.extra_args = config.extra_args;
        }
    }

    pub fn update(&mut self, action: Action, tx: &UnboundedSender<Action>) {
        match action {
            Action::Tick => self.auto_refresh_project(),
            Action::Quit => self.should_quit = true,
            Action::CharInput(ch) => self.handle_char_input(ch, tx),
            Action::Backspace => self.handle_backspace(),
            Action::NextView => {
                if self.inventory_wizard_open {
                    self.cycle_inventory_wizard_focus(1);
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(1);
                } else if !self.settings_editor_open
                    && !self.inventory_create_open
                    && !self.inventory_editor_open
                    && !self.inventory_edit_mode_open
                    && !self.runtime_prompt_open
                    && !(self.current_view() == View::Settings && self.global_settings_text_mode)
                {
                    self.view_idx = (self.view_idx + 1) % View::all().len();
                    if self.current_view() == View::Playbooks {
                        self.playbooks_focus_runs = false;
                        self.sync_run_selection_to_selected_playbook();
                    }
                }
            }
            Action::PrevView => {
                if self.inventory_wizard_open {
                    self.cycle_inventory_wizard_focus(-1);
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(-1);
                } else if !self.settings_editor_open
                    && !self.inventory_create_open
                    && !self.inventory_editor_open
                    && !self.inventory_edit_mode_open
                    && !self.runtime_prompt_open
                    && !(self.current_view() == View::Settings && self.global_settings_text_mode)
                {
                    self.view_idx = (self.view_idx + View::all().len() - 1) % View::all().len();
                    if self.current_view() == View::Playbooks {
                        self.playbooks_focus_runs = false;
                        self.sync_run_selection_to_selected_playbook();
                    }
                }
            }
            Action::SettingsIncrease => {
                if self.settings_editor_open {
                    self.adjust_settings_field(1);
                } else if self.inventory_wizard_open {
                    self.cycle_inventory_wizard_focus(1);
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(1);
                } else if self.current_view() == View::Playbooks && !self.runtime_prompt_open {
                    self.playbooks_focus_runs = true;
                } else {
                    self.adjust_global_settings_field(1);
                }
            }
            Action::SettingsDecrease => {
                if self.settings_editor_open {
                    self.adjust_settings_field(-1);
                } else if self.inventory_wizard_open {
                    self.cycle_inventory_wizard_focus(-1);
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(-1);
                } else if self.current_view() == View::Playbooks && !self.runtime_prompt_open {
                    self.playbooks_focus_runs = false;
                } else {
                    self.adjust_global_settings_field(-1);
                }
            }
            Action::MoveUp => {
                if self.settings_editor_open {
                    if !self.settings_editor_text_mode {
                        self.settings_editor_field_idx =
                            self.settings_editor_field_idx.saturating_sub(1);
                    }
                } else if self.inventory_create_open {
                } else if self.inventory_editor_open {
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(-1);
                } else if self.inventory_wizard_open {
                    self.move_inventory_wizard_selection(-1);
                } else if self.runtime_prompt_open {
                    self.runtime_candidate_idx = self.runtime_candidate_idx.saturating_sub(1);
                } else if self.current_view() == View::Settings {
                    if !self.global_settings_text_mode {
                        self.global_settings_field_idx =
                            self.global_settings_field_idx.saturating_sub(1);
                    }
                } else if self.log_select_mode {
                    self.move_log_cursor_up();
                } else {
                    self.move_selection_up();
                }
            }
            Action::MoveDown => {
                if self.settings_editor_open {
                    if !self.settings_editor_text_mode {
                        self.settings_editor_field_idx = min(
                            self.settings_editor_field_idx + 1,
                            PLAYBOOK_SETTINGS_FIELD_COUNT - 1,
                        );
                    }
                } else if self.inventory_create_open {
                } else if self.inventory_editor_open {
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(1);
                } else if self.inventory_wizard_open {
                    self.move_inventory_wizard_selection(1);
                } else if self.runtime_prompt_open {
                    if !self.runtime_candidates.is_empty() {
                        self.runtime_candidate_idx = min(
                            self.runtime_candidate_idx + 1,
                            self.runtime_candidates.len() - 1,
                        );
                    }
                } else if self.current_view() == View::Settings {
                    if !self.global_settings_text_mode {
                        self.global_settings_field_idx = min(
                            self.global_settings_field_idx + 1,
                            GLOBAL_SETTINGS_FIELD_COUNT - 1,
                        );
                    }
                } else if self.log_select_mode {
                    self.move_log_cursor_down();
                } else {
                    self.move_selection_down();
                }
            }
            Action::ToggleLogSelectMode => self.toggle_log_select_mode(),
            Action::MarkLogSelection => self.mark_log_selection(),
            Action::CopyLogSelection => self.copy_log_selection(),
            Action::LogMouseDown {
                row,
                viewport_height,
            } => self.log_mouse_down(row, viewport_height),
            Action::LogMouseDrag {
                row,
                viewport_height,
            } => self.log_mouse_drag(row, viewport_height),
            Action::LogMouseUp => self.log_mouse_up(),
            Action::ToggleCheckMode => self.toggle_check_mode(),
            Action::ToggleDiffMode => self.toggle_diff_mode(),
            Action::SaveInventoryEditor => {
                if self.inventory_wizard_open {
                    if self.inventory_wizard_input_mode.is_some() {
                        self.commit_inventory_wizard_input();
                    } else {
                        self.save_inventory_wizard();
                    }
                } else {
                    self.save_inventory_editor();
                }
            }
            Action::OpenRuntimePrompt => self.open_runtime_prompt(),
            Action::CloseRuntimePrompt => {
                if self.settings_editor_open && self.settings_editor_text_mode {
                    self.cancel_settings_text_edit();
                } else if self.settings_editor_open {
                    self.close_playbook_settings();
                } else if self.inventory_create_open {
                    self.cancel_inventory_create_prompt();
                } else if self.inventory_editor_open {
                    self.close_inventory_editor(false);
                } else if self.inventory_edit_mode_open {
                    self.close_inventory_edit_mode_prompt();
                } else if self.inventory_wizard_open {
                    self.close_inventory_wizard();
                } else if self.current_view() == View::Settings && self.global_settings_text_mode {
                    self.cancel_global_settings_text_edit();
                } else {
                    self.runtime_prompt_open = false;
                }
            }
            Action::SelectRuntimeCandidate => {
                if self.settings_editor_open {
                    self.confirm_settings_editor();
                } else if self.inventory_editor_open {
                    self.insert_inventory_editor_newline();
                } else if self.inventory_create_open {
                    self.confirm_inventory_create();
                } else if self.inventory_edit_mode_open {
                    self.confirm_inventory_edit_mode_selection();
                } else if self.inventory_wizard_open {
                    self.submit_inventory_wizard();
                } else if self.runtime_prompt_open {
                    self.select_runtime_candidate();
                } else if self.current_view() == View::Settings {
                    self.confirm_global_settings_editor();
                }
            }
            Action::BootstrapManagedRuntime => self.bootstrap_managed_runtime(tx),
            Action::RuntimeBootstrapLog(line) => self.record_runtime_log(line),
            Action::RuntimeBootstrapFinished {
                success,
                ansible_bin,
                message,
            } => {
                self.runtime_bootstrapping = false;
                self.record_runtime_log(message.clone());
                if success {
                    if let Some(ansible_bin) = ansible_bin {
                        self.run_options.ansible_bin = ansible_bin;
                        self.persist_runtime_config();
                        self.status_line =
                            String::from("Managed runtime ready and selected for execution");
                    }
                    self.refresh_runtime_candidates();
                    self.runtime_prompt_open = false;
                } else {
                    self.status_line = format!("Runtime bootstrap failed: {message}");
                    self.open_runtime_prompt();
                }
            }
            Action::RefreshProject => self.refresh_project(),
            Action::StartRun => self.start_run(tx),
            Action::RunStarted {
                run_id,
                playbook,
                inventory,
            } => {
                self.runs.insert(
                    0,
                    RunRecord {
                        id: run_id,
                        playbook,
                        inventory,
                        status: RunStatus::Running,
                        started_at: Local::now(),
                        finished_at: None,
                        exit_code: None,
                        logs: Vec::new(),
                    },
                );
                self.run_idx = 0;
                self.log_cursor = 0;
                self.log_anchor = None;
                self.remember_selected_run_for_current_playbook();
                self.status_line = format!("Run #{run_id} started");
                self.persist_run(run_id);
            }
            Action::RunLog { run_id, line } => {
                if let Some(idx) = self.runs.iter().position(|r| r.id == run_id) {
                    let run = &mut self.runs[idx];
                    run.logs.push(line);
                    if run.logs.len() > MAX_LOG_LINES {
                        let over = run.logs.len().saturating_sub(MAX_LOG_LINES);
                        run.logs.drain(0..over);
                    }
                    if !self.log_select_mode && self.run_idx == idx {
                        self.log_cursor = run.logs.len().saturating_sub(1);
                    }
                }
            }
            Action::RunFinished {
                run_id,
                success,
                exit_code,
            } => {
                if let Some(idx) = self.runs.iter().position(|r| r.id == run_id) {
                    let run = &mut self.runs[idx];
                    run.status = if success {
                        RunStatus::Succeeded
                    } else {
                        RunStatus::Failed
                    };
                    run.exit_code = exit_code;
                    run.finished_at = Some(Local::now());
                    self.status_line = match exit_code {
                        Some(code) => format!(
                            "Run #{run_id} {} (exit {code})",
                            if success { "succeeded" } else { "failed" }
                        ),
                        None => format!(
                            "Run #{run_id} {}",
                            if success { "succeeded" } else { "failed" }
                        ),
                    };
                    self.persist_run(run_id);
                }
            }
            Action::Error(err) => self.status_line = format!("Error: {err}"),
        }
    }

    fn handle_char_input(&mut self, ch: char, tx: &UnboundedSender<Action>) {
        if self.settings_editor_open && self.settings_editor_text_mode {
            self.push_settings_text_char(ch);
            return;
        }
        if self.inventory_create_open {
            self.push_inventory_create_char(ch);
            return;
        }
        if self.inventory_editor_open {
            self.push_inventory_editor_char(ch);
            return;
        }
        if self.inventory_edit_mode_open {
            self.handle_inventory_edit_mode_char(ch);
            return;
        }
        if self.inventory_wizard_open {
            self.handle_inventory_wizard_char(ch);
            return;
        }
        if self.current_view() == View::Settings && self.global_settings_text_mode {
            self.push_global_settings_text_char(ch);
            return;
        }

        if self.current_view() == View::Inventory
            && self.pending_inventory_delete.is_some()
            && ch != 'D'
        {
            self.pending_inventory_delete = None;
        }

        if ch == 'q' {
            self.should_quit = true;
            return;
        }

        if self.settings_editor_open {
            match ch {
                'j' => {
                    self.settings_editor_field_idx = min(
                        self.settings_editor_field_idx + 1,
                        PLAYBOOK_SETTINGS_FIELD_COUNT - 1,
                    );
                }
                'k' => {
                    self.settings_editor_field_idx =
                        self.settings_editor_field_idx.saturating_sub(1);
                }
                'h' => self.adjust_settings_field(-1),
                'l' => self.adjust_settings_field(1),
                ' ' => self.toggle_settings_boolean_field(),
                'e' => self.begin_settings_text_edit(),
                't' => self.close_playbook_settings(),
                _ => {}
            }
            return;
        }

        if self.runtime_prompt_open {
            match ch {
                'j' => {
                    if !self.runtime_candidates.is_empty() {
                        self.runtime_candidate_idx = min(
                            self.runtime_candidate_idx + 1,
                            self.runtime_candidates.len() - 1,
                        );
                    }
                }
                'k' => {
                    self.runtime_candidate_idx = self.runtime_candidate_idx.saturating_sub(1);
                }
                'b' => self.bootstrap_managed_runtime(tx),
                _ => {}
            }
            return;
        }

        if self.current_view() == View::Inventory {
            match ch {
                'n' => {
                    self.open_inventory_create_prompt();
                    return;
                }
                'e' => {
                    self.open_inventory_edit_mode_prompt();
                    return;
                }
                'g' => {
                    self.open_inventory_wizard();
                    return;
                }
                'D' => {
                    self.request_inventory_delete();
                    return;
                }
                'r' => {
                    self.status_line = String::from("Use Playbooks tab to run a selected playbook");
                    return;
                }
                _ => {}
            }
        }

        if self.current_view() == View::Playbooks {
            match ch {
                'i' => {
                    self.cycle_playbook_inventory(1);
                    return;
                }
                'I' => {
                    self.cycle_playbook_inventory(-1);
                    return;
                }
                _ => {}
            }
        }

        if self.current_view() == View::Settings {
            match ch {
                'j' => {
                    self.global_settings_field_idx = min(
                        self.global_settings_field_idx + 1,
                        GLOBAL_SETTINGS_FIELD_COUNT - 1,
                    );
                }
                'k' => {
                    self.global_settings_field_idx =
                        self.global_settings_field_idx.saturating_sub(1);
                }
                'h' => self.adjust_global_settings_field(-1),
                'l' => self.adjust_global_settings_field(1),
                ' ' => self.toggle_global_settings_boolean_field(),
                'e' => self.begin_global_settings_text_edit(),
                'u' => self.open_runtime_prompt(),
                'b' => self.bootstrap_managed_runtime(tx),
                _ => {}
            }
            return;
        }

        match ch {
            'h' => {
                self.view_idx = (self.view_idx + View::all().len() - 1) % View::all().len();
                if self.current_view() == View::Playbooks {
                    self.playbooks_focus_runs = false;
                    self.sync_run_selection_to_selected_playbook();
                }
            }
            'l' => {
                self.view_idx = (self.view_idx + 1) % View::all().len();
                if self.current_view() == View::Playbooks {
                    self.playbooks_focus_runs = false;
                    self.sync_run_selection_to_selected_playbook();
                }
            }
            'j' => {
                if self.log_select_mode {
                    self.move_log_cursor_down();
                } else {
                    self.move_selection_down();
                }
            }
            'k' => {
                if self.log_select_mode {
                    self.move_log_cursor_up();
                } else {
                    self.move_selection_up();
                }
            }
            'J' => self.select_playbook_run_offset(1),
            'K' => self.select_playbook_run_offset(-1),
            'r' => self.start_run(tx),
            'v' => self.toggle_log_select_mode(),
            'y' => self.copy_log_selection(),
            ' ' => self.mark_log_selection(),
            't' => self.open_playbook_settings(),
            'u' => self.open_runtime_prompt(),
            'b' => self.bootstrap_managed_runtime(tx),
            'c' => self.toggle_check_mode(),
            'd' => self.toggle_diff_mode(),
            'R' => self.refresh_project(),
            _ => {}
        }
    }

    fn handle_backspace(&mut self) {
        if self.settings_editor_open && self.settings_editor_text_mode {
            self.settings_editor_text_buffer.pop();
            return;
        }
        if self.inventory_create_open {
            self.inventory_create_buffer.pop();
            return;
        }
        if self.inventory_editor_open {
            self.inventory_editor_buffer.pop();
            self.inventory_editor_dirty = true;
            return;
        }
        if self.inventory_wizard_open {
            self.backspace_inventory_wizard();
            return;
        }
        if self.current_view() == View::Settings && self.global_settings_text_mode {
            self.global_settings_text_buffer.pop();
        }
    }

    fn confirm_settings_editor(&mut self) {
        if !self.settings_editor_open {
            return;
        }
        if self.settings_editor_text_mode {
            self.commit_settings_text_edit();
            return;
        }
        if self.settings_field_is_text() {
            self.begin_settings_text_edit();
            return;
        }
        self.close_playbook_settings();
    }

    fn settings_field_is_text(&self) -> bool {
        self.settings_editor_field_idx >= PLAYBOOK_SETTINGS_TEXT_FIELD_START
            && self.settings_editor_field_idx < PLAYBOOK_SETTINGS_FIELD_COUNT
    }

    fn begin_settings_text_edit(&mut self) {
        if !self.settings_editor_open || !self.settings_field_is_text() {
            return;
        }
        self.settings_editor_text_buffer = self.current_settings_text_value().unwrap_or_default();
        self.settings_editor_text_mode = true;
        self.status_line =
            String::from("Playbook settings: text edit mode ON (type, Backspace, Enter save)");
    }

    fn cancel_settings_text_edit(&mut self) {
        self.settings_editor_text_mode = false;
        self.settings_editor_text_buffer.clear();
        self.status_line = String::from("Playbook settings: text edit cancelled");
    }

    fn commit_settings_text_edit(&mut self) {
        if !self.settings_editor_text_mode {
            return;
        }
        let raw = self.settings_editor_text_buffer.trim();
        let value = if raw.is_empty() {
            None
        } else {
            Some(raw.to_string())
        };
        self.set_current_settings_text_value(value);
        self.settings_editor_text_mode = false;
        self.settings_editor_text_buffer.clear();
        self.persist_playbook_settings();
        self.status_line = String::from("Playbook settings updated");
    }

    fn push_settings_text_char(&mut self, ch: char) {
        if !self.settings_editor_text_mode {
            return;
        }
        if !ch.is_control() {
            self.settings_editor_text_buffer.push(ch);
        }
    }

    fn current_settings_text_value(&self) -> Option<String> {
        let settings = self.selected_playbook_settings()?;
        match self.settings_editor_field_idx {
            6 => settings.limit,
            7 => settings.tags,
            8 => settings.extra_vars,
            9 => settings.extra_args,
            _ => None,
        }
    }

    fn set_current_settings_text_value(&mut self, value: Option<String>) {
        let Some(key) = self.selected_playbook_key() else {
            return;
        };
        let Some(settings) = self.playbook_settings.get_mut(&key) else {
            return;
        };
        match self.settings_editor_field_idx {
            6 => settings.limit = value,
            7 => settings.tags = value,
            8 => settings.extra_vars = value,
            9 => settings.extra_args = value,
            _ => {}
        }
    }

    fn confirm_global_settings_editor(&mut self) {
        if self.global_settings_text_mode {
            self.commit_global_settings_text_edit();
            return;
        }
        if self.global_settings_field_is_text() {
            self.begin_global_settings_text_edit();
            return;
        }
        self.toggle_global_settings_boolean_field();
    }

    fn global_settings_field_is_text(&self) -> bool {
        matches!(self.global_settings_field_idx, 0 | 1 | 6 | 8 | 9 | 10)
    }

    fn begin_global_settings_text_edit(&mut self) {
        if self.runtime_prompt_open || !self.global_settings_field_is_text() {
            return;
        }
        self.global_settings_text_buffer = self.current_global_settings_text_value();
        self.global_settings_text_mode = true;
        self.status_line =
            String::from("Global settings: text edit mode ON (type, Backspace, Enter save)");
    }

    fn cancel_global_settings_text_edit(&mut self) {
        self.global_settings_text_mode = false;
        self.global_settings_text_buffer.clear();
        self.status_line = String::from("Global settings: text edit cancelled");
    }

    fn commit_global_settings_text_edit(&mut self) {
        if !self.global_settings_text_mode {
            return;
        }
        let value = self.global_settings_text_buffer.trim().to_string();
        self.set_current_global_settings_text_value(value);
        self.global_settings_text_mode = false;
        self.global_settings_text_buffer.clear();
        self.persist_global_settings();
        self.status_line = String::from("Global settings updated");
    }

    fn push_global_settings_text_char(&mut self, ch: char) {
        if !self.global_settings_text_mode {
            return;
        }
        if !ch.is_control() {
            self.global_settings_text_buffer.push(ch);
        }
    }

    fn current_global_settings_text_value(&self) -> String {
        match self.global_settings_field_idx {
            0 => self.run_options.ansible_bin.clone(),
            1 => self
                .ansible_cfg
                .interpreter_python
                .clone()
                .unwrap_or_default(),
            6 => self.ansible_cfg.stdout_callback.clone().unwrap_or_default(),
            8 => self
                .ansible_cfg
                .retry_files_save_path
                .clone()
                .unwrap_or_default(),
            9 => self.ansible_cfg.remote_user.clone().unwrap_or_default(),
            10 => self
                .ansible_cfg
                .private_key_file
                .clone()
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn set_current_global_settings_text_value(&mut self, value: String) {
        let value = value.trim().to_string();
        match self.global_settings_field_idx {
            0 => {
                if !value.is_empty() {
                    self.run_options.ansible_bin = value;
                    self.refresh_runtime_candidates();
                }
            }
            1 => self.ansible_cfg.interpreter_python = normalize_optional_text(value),
            6 => self.ansible_cfg.stdout_callback = normalize_optional_text(value),
            8 => self.ansible_cfg.retry_files_save_path = normalize_optional_text(value),
            9 => self.ansible_cfg.remote_user = normalize_optional_text(value),
            10 => self.ansible_cfg.private_key_file = normalize_optional_text(value),
            _ => {}
        }
    }

    fn toggle_global_settings_boolean_field(&mut self) {
        if self.runtime_prompt_open || self.global_settings_text_mode {
            return;
        }
        match self.global_settings_field_idx {
            5 => self.ansible_cfg.host_key_checking = !self.ansible_cfg.host_key_checking,
            7 => self.ansible_cfg.retry_files_enabled = !self.ansible_cfg.retry_files_enabled,
            11 => self.ansible_cfg.pipelining = !self.ansible_cfg.pipelining,
            _ => return,
        }
        self.persist_global_settings();
    }

    fn adjust_global_settings_field(&mut self, delta: i8) {
        if self.current_view() != View::Settings
            || self.runtime_prompt_open
            || self.global_settings_text_mode
        {
            return;
        }
        match self.global_settings_field_idx {
            2 => {
                self.ansible_cfg.forks = cycle_u16(
                    self.ansible_cfg.forks,
                    &[None, Some(5), Some(10), Some(20), Some(50)],
                    delta,
                );
                self.run_options.forks = self.ansible_cfg.forks;
            }
            3 => {
                self.ansible_cfg.timeout = cycle_u16(
                    self.ansible_cfg.timeout,
                    &[None, Some(10), Some(30), Some(60), Some(120)],
                    delta,
                );
                self.run_options.timeout = self.ansible_cfg.timeout;
            }
            4 => {
                let v = self.ansible_cfg.verbosity as i8 + delta;
                self.ansible_cfg.verbosity = max(0, min(4, v)) as u8;
                self.run_options.verbosity = self.ansible_cfg.verbosity;
            }
            5 => {
                self.ansible_cfg.host_key_checking = !self.ansible_cfg.host_key_checking;
            }
            7 => {
                self.ansible_cfg.retry_files_enabled = !self.ansible_cfg.retry_files_enabled;
            }
            11 => {
                self.ansible_cfg.pipelining = !self.ansible_cfg.pipelining;
            }
            _ => return,
        }
        self.persist_global_settings();
    }

    fn move_selection_up(&mut self) {
        match self.current_view() {
            View::Inventory => {
                self.inventory_idx = self.inventory_idx.saturating_sub(1);
                self.pending_inventory_delete = None;
            }
            View::Playbooks => {
                if self.playbooks_focus_runs {
                    self.move_selected_playbook_run(-1, false);
                } else {
                    self.playbook_idx = self.playbook_idx.saturating_sub(1);
                    self.sync_run_selection_to_selected_playbook();
                }
            }
            _ => {}
        }
    }

    fn move_selection_down(&mut self) {
        match self.current_view() {
            View::Inventory => {
                if !self.inventories.is_empty() {
                    self.inventory_idx = min(self.inventory_idx + 1, self.inventories.len() - 1);
                }
                self.pending_inventory_delete = None;
            }
            View::Playbooks => {
                if self.playbooks_focus_runs {
                    self.move_selected_playbook_run(1, false);
                } else {
                    if !self.playbooks.is_empty() {
                        self.playbook_idx = min(self.playbook_idx + 1, self.playbooks.len() - 1);
                    }
                    self.sync_run_selection_to_selected_playbook();
                }
            }
            _ => {}
        }
    }

    fn move_log_cursor_up(&mut self) {
        self.log_cursor = self.log_cursor.saturating_sub(1);
    }

    fn move_log_cursor_down(&mut self) {
        if let Some(run) = self.runs.get(self.run_idx) {
            if !run.logs.is_empty() {
                self.log_cursor = min(self.log_cursor + 1, run.logs.len() - 1);
            }
        }
    }

    fn toggle_log_select_mode(&mut self) {
        self.log_select_mode = !self.log_select_mode;
        self.log_anchor = None;
        if self.log_select_mode {
            self.sync_log_cursor_to_selected_run();
            self.status_line =
                String::from("Log select mode ON: j/k move, space mark, y copy, mouse drag select");
        } else {
            self.sync_log_cursor_to_selected_run();
            self.status_line = String::from("Log select mode OFF");
        }
    }

    fn mark_log_selection(&mut self) {
        if !self.log_select_mode {
            return;
        }
        if self.runs.get(self.run_idx).is_none() {
            self.status_line = String::from("No run selected");
            return;
        }
        self.log_anchor = match self.log_anchor {
            Some(_) => None,
            None => Some(self.log_cursor),
        };
        self.status_line = if self.log_anchor.is_some() {
            String::from("Log selection mark set")
        } else {
            String::from("Log selection mark cleared")
        };
    }

    fn copy_log_selection(&mut self) {
        let Some(run) = self.runs.get(self.run_idx) else {
            self.status_line = String::from("No run selected");
            return;
        };
        if run.logs.is_empty() {
            self.status_line = String::from("No log lines to copy");
            return;
        }

        let cursor = min(self.log_cursor, run.logs.len().saturating_sub(1));
        let anchor = min(
            self.log_anchor.unwrap_or(cursor),
            run.logs.len().saturating_sub(1),
        );
        let start = min(anchor, cursor);
        let end = anchor.max(cursor);
        let text = run.logs[start..=end].join("\n");

        match copy_text_to_clipboard(&text) {
            Ok(method) => {
                self.status_line = format!("Copied {} log line(s) via {}", end - start + 1, method);
            }
            Err(err) => {
                self.status_line = format!("Failed to copy logs: {err}");
            }
        }
    }

    fn log_mouse_down(&mut self, row: u16, viewport_height: u16) {
        if self.runtime_prompt_open || self.settings_editor_open || self.runs.is_empty() {
            return;
        }
        let Some(idx) = self.log_index_from_view_row(row, viewport_height) else {
            return;
        };
        self.log_select_mode = true;
        self.log_cursor = idx;
        self.log_anchor = Some(idx);
        self.status_line = String::from("Log selection started. Drag and release to copy.");
    }

    fn log_mouse_drag(&mut self, row: u16, viewport_height: u16) {
        if !self.log_select_mode || self.runs.is_empty() {
            return;
        }
        let Some(idx) = self.log_index_from_view_row(row, viewport_height) else {
            return;
        };
        self.log_cursor = idx;
    }

    fn log_mouse_up(&mut self) {
        if !self.log_select_mode || self.log_anchor.is_none() {
            return;
        }
        self.copy_log_selection();
    }

    fn toggle_check_mode(&mut self) {
        self.run_options.check = !self.run_options.check;
        self.persist_global_settings();
        self.status_line = format!(
            "check mode {}",
            if self.run_options.check {
                "enabled"
            } else {
                "disabled"
            }
        );
    }

    fn toggle_diff_mode(&mut self) {
        self.run_options.diff = !self.run_options.diff;
        self.persist_global_settings();
        self.status_line = format!(
            "diff mode {}",
            if self.run_options.diff {
                "enabled"
            } else {
                "disabled"
            }
        );
    }

    fn start_run(&mut self, tx: &UnboundedSender<Action>) {
        if self.settings_editor_open {
            self.status_line = String::from("Close playbook settings editor before running");
            return;
        }
        if self.runtime_bootstrapping {
            self.status_line = String::from("Runtime bootstrap in progress...");
            return;
        }
        if !playbook_bin_available(&self.run_options.ansible_bin) {
            self.status_line = format!(
                "{} not found. Press u to pick runtime or b to bootstrap managed runtime",
                self.run_options.ansible_bin
            );
            self.open_runtime_prompt();
            return;
        }
        if self.playbooks.is_empty() {
            self.status_line =
                String::from("No playbooks found. Add *.yml under ./playbooks or project root.");
            return;
        }
        if self.inventories.is_empty() {
            self.status_line =
                String::from("No inventories found. Add inventory files under ./inventories.");
            return;
        }

        let Some(playbook_path) = self.playbooks.get(self.playbook_idx) else {
            self.status_line = String::from("No playbook selected.");
            return;
        };
        let Some(inventory_path) = self.selected_inventory_path_for_current_playbook() else {
            self.status_line = String::from("No inventory selected.");
            return;
        };

        let run_id = self.next_run_id;
        self.next_run_id += 1;

        let playbook = display_path(&self.cwd, playbook_path);
        let inventory = display_path(&self.cwd, &inventory_path);
        let settings = self
            .playbook_settings
            .get(&playbook)
            .cloned()
            .unwrap_or_else(|| self.default_settings());

        let mut options = self.run_options.clone();
        options.check = settings.check;
        options.diff = settings.diff;
        options.become_enabled = settings.become_enabled;
        options.verbosity = settings.verbosity;
        options.forks = settings.forks;
        options.timeout = settings.timeout;
        options.limit = settings.limit;
        options.tags = settings.tags;
        options.extra_vars = settings.extra_vars;
        options.extra_args = settings.extra_args;

        self.status_line = format!("Starting run #{run_id}...");
        spawn_ansible_run(
            RunRequest {
                run_id,
                cwd: self.cwd.clone(),
                playbook,
                inventory,
                options,
            },
            tx.clone(),
        );
    }

    fn refresh_project(&mut self) {
        let (inventories, playbooks) = discover_project(&self.cwd);
        self.apply_discovered_project(inventories, playbooks);
        self.status_line = format!(
            "Loaded {} playbooks and {} inventories",
            self.playbooks.len(),
            self.inventories.len()
        );
    }

    fn auto_refresh_project(&mut self) {
        if self.last_auto_discovery_at.elapsed() < AUTO_DISCOVERY_INTERVAL {
            return;
        }
        self.last_auto_discovery_at = Instant::now();
        let (inventories, playbooks) = discover_project(&self.cwd);
        if inventories == self.inventories && playbooks == self.playbooks {
            return;
        }
        self.apply_discovered_project(inventories, playbooks);
        self.status_line = format!(
            "Project updated: {} playbooks, {} inventories",
            self.playbooks.len(),
            self.inventories.len()
        );
    }

    fn apply_discovered_project(&mut self, inventories: Vec<PathBuf>, playbooks: Vec<PathBuf>) {
        self.inventories = inventories;
        self.playbooks = playbooks;
        if self.inventory_idx >= self.inventories.len() {
            self.inventory_idx = self.inventories.len().saturating_sub(1);
        }
        if self.playbook_idx >= self.playbooks.len() {
            self.playbook_idx = self.playbooks.len().saturating_sub(1);
        }
        if let Some(path) = &self.pending_inventory_delete {
            if !self.inventories.iter().any(|p| p == path) {
                self.pending_inventory_delete = None;
            }
        }
        let known_playbooks = self
            .playbooks
            .iter()
            .map(|p| display_path(&self.cwd, p))
            .collect::<HashSet<_>>();
        self.selected_run_by_playbook
            .retain(|playbook, _| known_playbooks.contains(playbook));
        self.selected_inventory_by_playbook
            .retain(|playbook, inventory| {
                known_playbooks.contains(playbook)
                    && self
                        .inventories
                        .iter()
                        .any(|path| display_path(&self.cwd, path) == *inventory)
            });
        self.ensure_settings_for_playbooks();
        self.sync_run_selection_to_selected_playbook();
    }

    fn open_inventory_editor(&mut self) {
        if self.current_view() != View::Inventory {
            self.status_line = String::from("Inventory editor is available in Inventory tab");
            return;
        }
        let Some(path) = self.inventories.get(self.inventory_idx).cloned() else {
            self.status_line = String::from("No inventory selected");
            return;
        };
        match fs::read_to_string(&path) {
            Ok(content) => {
                self.inventory_editor_open = true;
                self.inventory_editor_buffer = content;
                self.inventory_editor_path = Some(path.clone());
                self.inventory_editor_dirty = false;
                self.pending_inventory_delete = None;
                self.status_line = format!(
                    "Editing inventory {} (Ctrl+S save, Esc close)",
                    display_path(&self.cwd, &path)
                );
            }
            Err(err) => {
                self.status_line = format!("Failed to open inventory for edit: {err}");
            }
        }
    }

    fn push_inventory_editor_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        self.inventory_editor_buffer.push(ch);
        self.inventory_editor_dirty = true;
    }

    fn insert_inventory_editor_newline(&mut self) {
        if !self.inventory_editor_open {
            return;
        }
        self.inventory_editor_buffer.push('\n');
        self.inventory_editor_dirty = true;
    }

    fn save_inventory_editor(&mut self) {
        if !self.inventory_editor_open {
            return;
        }
        let Some(path) = self.inventory_editor_path.clone() else {
            self.status_line = String::from("No inventory file open in editor");
            return;
        };
        match fs::write(&path, &self.inventory_editor_buffer) {
            Ok(_) => {
                self.inventory_editor_dirty = false;
                self.refresh_project();
                self.status_line = format!("Saved inventory {}", display_path(&self.cwd, &path));
            }
            Err(err) => {
                self.status_line = format!("Failed to save inventory: {err}");
            }
        }
    }

    fn close_inventory_editor(&mut self, save_first: bool) {
        if !self.inventory_editor_open {
            return;
        }
        if save_first {
            self.save_inventory_editor();
        }
        let was_dirty = self.inventory_editor_dirty;
        self.inventory_editor_open = false;
        self.inventory_editor_buffer.clear();
        self.inventory_editor_path = None;
        self.inventory_editor_dirty = false;
        self.status_line = if was_dirty && !save_first {
            String::from("Closed inventory editor and discarded unsaved changes")
        } else {
            String::from("Closed inventory editor")
        };
    }

    fn open_inventory_edit_mode_prompt(&mut self) {
        if self.current_view() != View::Inventory {
            self.status_line = String::from("Inventory edit is available in Inventory tab");
            return;
        }
        if self.inventories.get(self.inventory_idx).is_none() {
            self.status_line = String::from("No inventory selected");
            return;
        }
        self.inventory_edit_mode_open = true;
        self.inventory_edit_mode_idx = 0;
        self.status_line = String::from("Choose inventory edit mode");
    }

    fn close_inventory_edit_mode_prompt(&mut self) {
        self.inventory_edit_mode_open = false;
        self.inventory_edit_mode_idx = 0;
        self.status_line = String::from("Inventory edit mode selection cancelled");
    }

    fn move_inventory_edit_mode_selection(&mut self, delta: i8) {
        if !self.inventory_edit_mode_open {
            return;
        }
        const MODE_COUNT: usize = 3;
        if delta.is_positive() {
            self.inventory_edit_mode_idx = min(self.inventory_edit_mode_idx + 1, MODE_COUNT - 1);
        } else {
            self.inventory_edit_mode_idx = self.inventory_edit_mode_idx.saturating_sub(1);
        }
    }

    fn handle_inventory_edit_mode_char(&mut self, ch: char) {
        match ch {
            'j' => self.move_inventory_edit_mode_selection(1),
            'k' => self.move_inventory_edit_mode_selection(-1),
            '1' => self.inventory_edit_mode_idx = 0,
            '2' => self.inventory_edit_mode_idx = 1,
            '3' => self.inventory_edit_mode_idx = 2,
            'g' => {
                self.inventory_edit_mode_idx = 0;
                self.confirm_inventory_edit_mode_selection();
            }
            'e' => {
                self.inventory_edit_mode_idx = 1;
                self.confirm_inventory_edit_mode_selection();
            }
            't' => {
                self.inventory_edit_mode_idx = 2;
                self.confirm_inventory_edit_mode_selection();
            }
            _ => {}
        }
    }

    fn confirm_inventory_edit_mode_selection(&mut self) {
        if !self.inventory_edit_mode_open {
            return;
        }

        match self.inventory_edit_mode_idx {
            0 => {
                self.open_inventory_wizard_for_selected_inventory();
                if self.inventory_wizard_open {
                    self.inventory_edit_mode_open = false;
                    self.inventory_edit_mode_idx = 0;
                }
            }
            1 => {
                self.inventory_edit_mode_open = false;
                self.inventory_edit_mode_idx = 0;
                self.open_inventory_external_editor();
            }
            _ => {
                self.inventory_edit_mode_open = false;
                self.inventory_edit_mode_idx = 0;
                self.open_inventory_editor();
            }
        }
    }

    fn open_inventory_external_editor(&mut self) {
        if self.current_view() != View::Inventory {
            self.status_line = String::from("External editor is available in Inventory tab");
            return;
        }
        let Some(path) = self.inventories.get(self.inventory_idx).cloned() else {
            self.status_line = String::from("No inventory selected");
            return;
        };

        let editor = std::env::var("VISUAL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                std::env::var("EDITOR")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| String::from("vim"));
        let escaped_path = shell_escape_single_quoted(&path.to_string_lossy());
        let command = format!("exec {editor} '{escaped_path}'");

        set_input_paused(true);

        if let Err(err) = suspend_tui_for_external_process() {
            set_input_paused(false);
            self.status_line = format!("Failed to suspend terminal for external editor: {err}");
            return;
        }

        let editor_status = Command::new("sh").arg("-lc").arg(&command).status();

        let restore_err = resume_tui_after_external_process().err();
        if restore_err.is_none() {
            drain_stale_terminal_input(Duration::from_millis(180));
        }
        set_input_paused(false);

        if let Some(err) = restore_err {
            self.status_line =
                format!("External editor closed, but terminal restore failed: {err}");
            return;
        }
        self.needs_full_redraw = true;

        match editor_status {
            Ok(status) if status.success() => {
                self.refresh_project();
                self.status_line = format!(
                    "External editor finished: {}",
                    display_path(&self.cwd, &path)
                );
            }
            Ok(status) => {
                let code = status
                    .code()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| String::from("terminated by signal"));
                self.status_line = format!("External editor exited with {code}");
            }
            Err(err) => {
                self.status_line = format!("Failed to launch external editor ({editor}): {err}");
            }
        }
    }

    fn open_inventory_wizard(&mut self) {
        if self.current_view() != View::Inventory {
            self.status_line = String::from("Inventory builder is available in Inventory tab");
            return;
        }
        self.inventory_wizard_edit_path = None;
        self.inventory_wizard_open = true;
        self.inventory_wizard_filename = String::from("inventory.yml");
        self.inventory_wizard_hosts = vec![String::from("localhost")];
        self.inventory_wizard_groups = vec![String::from("web"), String::from("db")];
        self.inventory_wizard_assignments = BTreeMap::from([
            (String::from("web"), vec![String::from("localhost")]),
            (String::from("db"), Vec::new()),
        ]);
        self.inventory_wizard_group_children = BTreeMap::from([
            (String::from("web"), Vec::new()),
            (String::from("db"), Vec::new()),
        ]);
        self.inventory_wizard_focus = InventoryWizardFocus::Tree;
        self.inventory_wizard_target_group = None;
        self.inventory_wizard_tree_idx = 0;
        self.inventory_wizard_host_idx = 0;
        self.inventory_wizard_group_idx = 0;
        self.inventory_wizard_input_mode = None;
        self.inventory_wizard_input_buffer.clear();
        self.pending_inventory_delete = None;
        self.sync_inventory_wizard_tree_selection();
        self.status_line = String::from(
            "Builder: pick target in Group Tree, then attach groups/hosts from right pane (Space).",
        );
    }

    fn open_inventory_wizard_for_selected_inventory(&mut self) {
        if self.current_view() != View::Inventory {
            self.status_line = String::from("Guided editor is available in Inventory tab");
            return;
        }
        let Some(path) = self.inventories.get(self.inventory_idx).cloned() else {
            self.status_line = String::from("No inventory selected");
            return;
        };
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(ext.as_str(), "yml" | "yaml") {
            self.status_line = format!(
                "Guided editor supports YAML inventories only: {}",
                display_path(&self.cwd, &path)
            );
            return;
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                self.status_line = format!("Failed to open inventory for guided edit: {err}");
                return;
            }
        };
        let parsed = match parse_inventory_yaml_for_builder(&content) {
            Ok(parsed) => parsed,
            Err(err) => {
                self.status_line = format!(
                    "Guided edit parse failed ({err}). Use text editor mode for this file."
                );
                return;
            }
        };

        self.inventory_wizard_edit_path = Some(path.clone());
        self.inventory_wizard_open = true;
        self.inventory_wizard_filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| String::from("inventory.yml"));
        self.inventory_wizard_hosts = parsed.hosts;
        self.inventory_wizard_groups = parsed.groups;
        self.inventory_wizard_assignments = parsed.assignments;
        self.inventory_wizard_group_children = parsed.group_children;
        self.inventory_wizard_focus = InventoryWizardFocus::Tree;
        self.inventory_wizard_target_group = None;
        self.inventory_wizard_tree_idx = 0;
        self.inventory_wizard_host_idx = 0;
        self.inventory_wizard_group_idx = 0;
        self.inventory_wizard_input_mode = None;
        self.inventory_wizard_input_buffer.clear();
        self.pending_inventory_delete = None;
        self.sync_inventory_wizard_tree_selection();
        self.sync_inventory_wizard_selection_bounds();
        self.status_line = format!("Guided editing {}", display_path(&self.cwd, &path));
    }

    fn close_inventory_wizard(&mut self) {
        if !self.inventory_wizard_open {
            return;
        }
        if self.inventory_wizard_input_mode.is_some() {
            self.inventory_wizard_input_mode = None;
            self.inventory_wizard_input_buffer.clear();
            self.status_line = String::from("Builder input cancelled");
            return;
        }
        self.inventory_wizard_open = false;
        self.inventory_wizard_filename.clear();
        self.inventory_wizard_edit_path = None;
        self.inventory_wizard_hosts.clear();
        self.inventory_wizard_groups.clear();
        self.inventory_wizard_assignments.clear();
        self.inventory_wizard_group_children.clear();
        self.inventory_wizard_focus = InventoryWizardFocus::Tree;
        self.inventory_wizard_target_group = None;
        self.inventory_wizard_tree_idx = 0;
        self.inventory_wizard_host_idx = 0;
        self.inventory_wizard_group_idx = 0;
        self.inventory_wizard_input_mode = None;
        self.inventory_wizard_input_buffer.clear();
        self.status_line = String::from("Closed guided inventory builder");
    }

    fn cycle_inventory_wizard_focus(&mut self, delta: i8) {
        if self.inventory_wizard_input_mode.is_some() {
            return;
        }
        self.inventory_wizard_focus = match (self.inventory_wizard_focus, delta.is_positive()) {
            (InventoryWizardFocus::Tree, true) => InventoryWizardFocus::Groups,
            (InventoryWizardFocus::Groups, true) => InventoryWizardFocus::Hosts,
            (InventoryWizardFocus::Hosts, true) => InventoryWizardFocus::Tree,
            (InventoryWizardFocus::Tree, false) => InventoryWizardFocus::Hosts,
            (InventoryWizardFocus::Groups, false) => InventoryWizardFocus::Tree,
            (InventoryWizardFocus::Hosts, false) => InventoryWizardFocus::Groups,
        };
        self.sync_inventory_wizard_selection_bounds();
    }

    fn move_inventory_wizard_selection(&mut self, delta: i8) {
        if self.inventory_wizard_input_mode.is_some() {
            return;
        }
        match self.inventory_wizard_focus {
            InventoryWizardFocus::Tree => {
                let entries = self.inventory_wizard_tree_nodes();
                if entries.is_empty() {
                    self.inventory_wizard_tree_idx = 0;
                    self.inventory_wizard_target_group = None;
                    return;
                }
                if delta.is_positive() {
                    self.inventory_wizard_tree_idx =
                        min(self.inventory_wizard_tree_idx + 1, entries.len() - 1);
                } else {
                    self.inventory_wizard_tree_idx =
                        self.inventory_wizard_tree_idx.saturating_sub(1);
                }
                self.inventory_wizard_target_group =
                    entries[self.inventory_wizard_tree_idx].0.clone();
                self.sync_inventory_wizard_selection_bounds();
            }
            InventoryWizardFocus::Groups => {
                let groups = self.inventory_wizard_candidate_groups();
                if groups.is_empty() {
                    self.inventory_wizard_group_idx = 0;
                    return;
                }
                if delta.is_positive() {
                    self.inventory_wizard_group_idx =
                        min(self.inventory_wizard_group_idx + 1, groups.len() - 1);
                } else {
                    self.inventory_wizard_group_idx =
                        self.inventory_wizard_group_idx.saturating_sub(1);
                }
            }
            InventoryWizardFocus::Hosts => {
                let hosts = self.inventory_wizard_candidate_hosts();
                if hosts.is_empty() {
                    self.inventory_wizard_host_idx = 0;
                    return;
                }
                if delta.is_positive() {
                    self.inventory_wizard_host_idx =
                        min(self.inventory_wizard_host_idx + 1, hosts.len() - 1);
                } else {
                    self.inventory_wizard_host_idx =
                        self.inventory_wizard_host_idx.saturating_sub(1);
                }
            }
        }
    }

    fn handle_inventory_wizard_char(&mut self, ch: char) {
        if self.inventory_wizard_input_mode.is_some() {
            if !ch.is_control() {
                self.inventory_wizard_input_buffer.push(ch);
            }
            return;
        }

        match ch {
            'n' | 'a' => self.begin_inventory_wizard_add(),
            'f' => self.begin_inventory_wizard_filename_edit(),
            'D' => self.delete_selected_inventory_wizard_item(),
            ' ' | 'c' => self.toggle_inventory_wizard_attachment_for_focus(),
            'd' => self.detach_inventory_wizard_attachment_for_focus(),
            's' => self.save_inventory_wizard(),
            'h' => self.cycle_inventory_wizard_focus(-1),
            'l' => self.cycle_inventory_wizard_focus(1),
            _ => {}
        }
    }

    fn backspace_inventory_wizard(&mut self) {
        if self.inventory_wizard_input_mode.is_some() {
            self.inventory_wizard_input_buffer.pop();
        }
    }

    fn submit_inventory_wizard(&mut self) {
        if !self.inventory_wizard_open {
            return;
        }
        if self.inventory_wizard_input_mode.is_some() {
            self.commit_inventory_wizard_input();
            return;
        }
        self.toggle_inventory_wizard_attachment_for_focus();
    }

    fn begin_inventory_wizard_filename_edit(&mut self) {
        self.inventory_wizard_input_mode = Some(InventoryWizardInputMode::Filename);
        self.inventory_wizard_input_buffer = self.inventory_wizard_filename.clone();
        self.status_line = String::from("Builder filename edit: type path and press Enter");
    }

    fn begin_inventory_wizard_add(&mut self) {
        match self.inventory_wizard_focus {
            InventoryWizardFocus::Hosts => {
                self.inventory_wizard_input_mode = Some(InventoryWizardInputMode::AddHost);
                self.inventory_wizard_input_buffer.clear();
                self.status_line = String::from("Builder add host: type host and press Enter");
            }
            InventoryWizardFocus::Tree | InventoryWizardFocus::Groups => {
                self.inventory_wizard_input_mode = Some(InventoryWizardInputMode::AddGroup);
                self.inventory_wizard_input_buffer.clear();
                self.status_line = String::from("Builder add group: type group and press Enter");
            }
        }
    }

    fn commit_inventory_wizard_input(&mut self) {
        let Some(mode) = self.inventory_wizard_input_mode else {
            return;
        };
        let value = self.inventory_wizard_input_buffer.trim().to_string();
        if value.is_empty() {
            self.status_line = String::from("Builder: value cannot be empty");
            return;
        }

        match mode {
            InventoryWizardInputMode::Filename => {
                let Some(filename) = normalize_inventory_filename(&value) else {
                    self.status_line =
                        String::from("Builder: filename must be plain and end with .yml/.yaml");
                    return;
                };
                self.inventory_wizard_filename = filename;
                self.status_line = String::from("Builder filename updated");
            }
            InventoryWizardInputMode::AddHost => {
                if !is_valid_inventory_key(&value) {
                    self.status_line = String::from(
                        "Builder: host supports letters, numbers, '.', '-', '_' and ':'",
                    );
                    return;
                }
                if self.inventory_wizard_hosts.contains(&value) {
                    self.status_line = String::from("Builder: host already exists");
                    return;
                }
                self.inventory_wizard_hosts.push(value);
                self.inventory_wizard_host_idx =
                    self.inventory_wizard_hosts.len().saturating_sub(1);
                self.status_line = String::from("Builder host added");
            }
            InventoryWizardInputMode::AddGroup => {
                if is_reserved_inventory_group(&value) || !is_valid_inventory_key(&value) {
                    self.status_line = String::from(
                        "Builder: group must be valid and cannot be reserved (all, ungrouped)",
                    );
                    return;
                }
                if self.inventory_wizard_groups.contains(&value) {
                    self.status_line = String::from("Builder: group already exists");
                    return;
                }
                self.inventory_wizard_groups.push(value.clone());
                self.inventory_wizard_target_group = Some(value.clone());
                self.inventory_wizard_assignments
                    .entry(value.clone())
                    .or_default();
                self.inventory_wizard_group_children
                    .entry(value)
                    .or_default();
                self.status_line = String::from("Builder group added");
                self.sync_inventory_wizard_tree_selection();
            }
        }
        self.inventory_wizard_input_mode = None;
        self.inventory_wizard_input_buffer.clear();
        self.sync_inventory_wizard_selection_bounds();
    }

    fn delete_selected_inventory_wizard_item(&mut self) {
        match self.inventory_wizard_focus {
            InventoryWizardFocus::Hosts => {
                let Some(removed) = self.selected_inventory_wizard_host() else {
                    self.status_line = String::from("Builder: no host selected");
                    return;
                };
                if let Some(idx) = self
                    .inventory_wizard_hosts
                    .iter()
                    .position(|h| h == &removed)
                {
                    self.inventory_wizard_hosts.remove(idx);
                }
                for assigned in self.inventory_wizard_assignments.values_mut() {
                    assigned.retain(|h| h != &removed);
                }
                self.status_line = format!("Builder host removed: {removed}");
            }
            InventoryWizardFocus::Tree | InventoryWizardFocus::Groups => {
                let group_to_remove = if self.inventory_wizard_focus == InventoryWizardFocus::Tree {
                    self.inventory_wizard_target_group.clone()
                } else {
                    self.selected_inventory_wizard_group_candidate()
                };
                let Some(removed) = group_to_remove else {
                    self.status_line = String::from("Builder: choose a group first");
                    return;
                };
                if let Some(pos) = self
                    .inventory_wizard_groups
                    .iter()
                    .position(|g| g == &removed)
                {
                    self.inventory_wizard_groups.remove(pos);
                } else {
                    self.status_line = String::from("Builder: selected group no longer exists");
                    return;
                }
                self.inventory_wizard_assignments.remove(&removed);
                self.inventory_wizard_group_children.remove(&removed);
                for children in self.inventory_wizard_group_children.values_mut() {
                    children.retain(|group| group != &removed);
                }
                if self.inventory_wizard_target_group.as_deref() == Some(removed.as_str()) {
                    self.inventory_wizard_target_group = None;
                }
                self.status_line = format!("Builder group removed: {removed}");
            }
        }
        self.sync_inventory_wizard_tree_selection();
        self.sync_inventory_wizard_selection_bounds();
    }

    fn toggle_inventory_wizard_attachment_for_focus(&mut self) {
        match self.inventory_wizard_focus {
            InventoryWizardFocus::Tree => {
                self.status_line =
                    String::from("Builder: move focus to Available Groups/Hosts to attach");
            }
            InventoryWizardFocus::Groups => self.toggle_inventory_wizard_group_for_target(),
            InventoryWizardFocus::Hosts => self.toggle_inventory_wizard_host_for_target(),
        }
    }

    fn detach_inventory_wizard_attachment_for_focus(&mut self) {
        match self.inventory_wizard_focus {
            InventoryWizardFocus::Tree => {
                self.status_line =
                    String::from("Builder: move focus to Available Groups/Hosts to detach");
            }
            InventoryWizardFocus::Groups => self.detach_inventory_wizard_group_for_target(),
            InventoryWizardFocus::Hosts => self.detach_inventory_wizard_host_for_target(),
        }
    }

    fn toggle_inventory_wizard_host_for_target(&mut self) {
        let Some(host) = self.selected_inventory_wizard_host() else {
            self.status_line = String::from("Builder: select a host first");
            return;
        };
        if let Some(target_group) = self.inventory_wizard_target_group.clone() {
            let entry = self
                .inventory_wizard_assignments
                .entry(target_group.clone())
                .or_default();
            if let Some(pos) = entry.iter().position(|h| h == &host) {
                entry.remove(pos);
                self.status_line = format!("Builder removed host {host} from {target_group}");
            } else {
                entry.push(host.clone());
                self.status_line = format!("Builder added host {host} to {target_group}");
            }
            return;
        }

        let mut removed = false;
        for assigned in self.inventory_wizard_assignments.values_mut() {
            let before = assigned.len();
            assigned.retain(|h| h != &host);
            removed |= assigned.len() != before;
        }
        if removed {
            self.status_line = format!("Builder moved host {host} to ungrouped (all)");
        } else {
            self.status_line = format!("Builder host {host} is already ungrouped");
        }
    }

    fn detach_inventory_wizard_host_for_target(&mut self) {
        let Some(host) = self.selected_inventory_wizard_host() else {
            self.status_line = String::from("Builder: select a host first");
            return;
        };
        if let Some(target_group) = self.inventory_wizard_target_group.clone() {
            let mut removed = false;
            if let Some(entry) = self.inventory_wizard_assignments.get_mut(&target_group) {
                let before = entry.len();
                entry.retain(|h| h != &host);
                removed = entry.len() != before;
            }
            self.status_line = if removed {
                format!("Builder removed host {host} from {target_group}")
            } else {
                format!("Builder host {host} is not in {target_group}")
            };
            return;
        }
        let mut removed = false;
        for assigned in self.inventory_wizard_assignments.values_mut() {
            let before = assigned.len();
            assigned.retain(|h| h != &host);
            removed |= assigned.len() != before;
        }
        self.status_line = if removed {
            format!("Builder moved host {host} to ungrouped (all)")
        } else {
            format!("Builder host {host} is already ungrouped")
        };
    }

    fn toggle_inventory_wizard_group_for_target(&mut self) {
        let Some(child) = self.selected_inventory_wizard_group_candidate() else {
            self.status_line = String::from("Builder: select a group first");
            return;
        };

        if let Some(parent) = self.inventory_wizard_target_group.clone() {
            if child == parent {
                self.status_line = String::from("Builder: group cannot be child of itself");
                return;
            }
            let linked = self
                .inventory_wizard_group_children
                .get(&parent)
                .map(|children| children.contains(&child))
                .unwrap_or(false);
            if linked {
                if let Some(children) = self.inventory_wizard_group_children.get_mut(&parent) {
                    children.retain(|group| group != &child);
                }
                self.status_line = format!("Builder unlinked {child} from {parent}");
                self.sync_inventory_wizard_tree_selection();
                return;
            }

            let mut prospective = self.inventory_wizard_group_children.clone();
            for children in prospective.values_mut() {
                children.retain(|group| group != &child);
            }
            prospective
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
            if group_children_has_cycle(&prospective) {
                self.status_line = String::from("Builder: link would create a cycle");
                return;
            }

            self.remove_inventory_wizard_child_from_all_parents(&child);
            self.inventory_wizard_group_children
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
            self.status_line = format!("Builder linked {child} under {parent}");
            self.sync_inventory_wizard_tree_selection();
            return;
        }

        if self.remove_inventory_wizard_child_from_all_parents(&child) {
            self.status_line = format!("Builder moved {child} to root under all");
        } else {
            self.status_line = format!("Builder group {child} is already at root");
        }
        self.sync_inventory_wizard_tree_selection();
    }

    fn detach_inventory_wizard_group_for_target(&mut self) {
        let Some(child) = self.selected_inventory_wizard_group_candidate() else {
            self.status_line = String::from("Builder: select a group first");
            return;
        };
        if let Some(parent) = self.inventory_wizard_target_group.clone() {
            let mut removed = false;
            if let Some(children) = self.inventory_wizard_group_children.get_mut(&parent) {
                let before = children.len();
                children.retain(|group| group != &child);
                removed = children.len() != before;
            }
            self.status_line = if removed {
                format!("Builder unlinked {child} from {parent}")
            } else {
                format!("Builder group {child} is not under {parent}")
            };
            self.sync_inventory_wizard_tree_selection();
            return;
        }
        if self.remove_inventory_wizard_child_from_all_parents(&child) {
            self.status_line = format!("Builder moved {child} to root under all");
        } else {
            self.status_line = format!("Builder group {child} is already at root");
        }
        self.sync_inventory_wizard_tree_selection();
    }

    fn remove_inventory_wizard_child_from_all_parents(&mut self, child: &str) -> bool {
        let mut removed = false;
        for children in self.inventory_wizard_group_children.values_mut() {
            let before = children.len();
            children.retain(|group| group != child);
            removed |= children.len() != before;
        }
        removed
    }

    fn selected_inventory_wizard_host(&self) -> Option<String> {
        self.inventory_wizard_candidate_hosts()
            .get(self.inventory_wizard_host_idx)
            .cloned()
    }

    fn selected_inventory_wizard_group_candidate(&self) -> Option<String> {
        self.inventory_wizard_candidate_groups()
            .get(self.inventory_wizard_group_idx)
            .cloned()
    }

    fn inventory_wizard_candidate_hosts(&self) -> Vec<String> {
        self.inventory_wizard_hosts.clone()
    }

    pub fn inventory_wizard_candidate_groups(&self) -> Vec<String> {
        self.inventory_wizard_groups
            .iter()
            .filter(|group| {
                self.inventory_wizard_target_group
                    .as_ref()
                    .map(|target| target != *group)
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>()
    }

    fn sync_inventory_wizard_selection_bounds(&mut self) {
        let groups_len = self.inventory_wizard_candidate_groups().len();
        if groups_len == 0 {
            self.inventory_wizard_group_idx = 0;
        } else if self.inventory_wizard_group_idx >= groups_len {
            self.inventory_wizard_group_idx = groups_len - 1;
        }

        let hosts_len = self.inventory_wizard_candidate_hosts().len();
        if hosts_len == 0 {
            self.inventory_wizard_host_idx = 0;
        } else if self.inventory_wizard_host_idx >= hosts_len {
            self.inventory_wizard_host_idx = hosts_len - 1;
        }
    }

    fn sync_inventory_wizard_tree_selection(&mut self) {
        let entries = self.inventory_wizard_tree_nodes();
        if entries.is_empty() {
            self.inventory_wizard_tree_idx = 0;
            self.inventory_wizard_target_group = None;
            return;
        }
        if let Some(pos) = entries
            .iter()
            .position(|(group, _)| *group == self.inventory_wizard_target_group)
        {
            self.inventory_wizard_tree_idx = pos;
        } else {
            self.inventory_wizard_tree_idx = 0;
            self.inventory_wizard_target_group = entries[0].0.clone();
        }
    }

    fn push_inventory_wizard_tree_node(
        &self,
        node: &str,
        depth: usize,
        known: &HashSet<String>,
        visited: &mut HashSet<String>,
        out: &mut Vec<(Option<String>, usize)>,
    ) {
        if !known.contains(node) || !visited.insert(node.to_string()) {
            return;
        }
        out.push((Some(node.to_string()), depth));
        if let Some(children) = self.inventory_wizard_group_children.get(node) {
            for child in children {
                self.push_inventory_wizard_tree_node(child, depth + 1, known, visited, out);
            }
        }
    }

    pub fn inventory_wizard_tree_nodes(&self) -> Vec<(Option<String>, usize)> {
        let mut out = vec![(None, 0)];
        if self.inventory_wizard_groups.is_empty() {
            return out;
        }

        let known = self
            .inventory_wizard_groups
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        let mut parent_of = BTreeMap::new();
        for parent in &self.inventory_wizard_groups {
            if let Some(children) = self.inventory_wizard_group_children.get(parent) {
                for child in children {
                    if known.contains(child) {
                        parent_of
                            .entry(child.clone())
                            .or_insert_with(|| parent.clone());
                    }
                }
            }
        }

        let roots = self
            .inventory_wizard_groups
            .iter()
            .filter(|group| !parent_of.contains_key(*group))
            .cloned()
            .collect::<Vec<_>>();

        let mut visited = HashSet::new();
        for root in roots {
            self.push_inventory_wizard_tree_node(&root, 1, &known, &mut visited, &mut out);
        }
        for group in &self.inventory_wizard_groups {
            if visited.insert(group.clone()) {
                out.push((Some(group.clone()), 1));
            }
        }
        out
    }

    pub fn inventory_wizard_input_prompt(&self) -> Option<&'static str> {
        match self.inventory_wizard_input_mode {
            Some(InventoryWizardInputMode::Filename) => Some("Filename"),
            Some(InventoryWizardInputMode::AddHost) => Some("New Host"),
            Some(InventoryWizardInputMode::AddGroup) => Some("New Group"),
            None => None,
        }
    }

    pub fn inventory_wizard_target_group_path(&self) -> String {
        let Some(target) = self.inventory_wizard_target_group.clone() else {
            return String::from("all");
        };

        let mut parent_of = BTreeMap::new();
        for parent in &self.inventory_wizard_groups {
            if let Some(children) = self.inventory_wizard_group_children.get(parent) {
                for child in children {
                    parent_of
                        .entry(child.clone())
                        .or_insert_with(|| parent.clone());
                }
            }
        }

        let mut path = vec![target.clone()];
        let mut cursor = target;
        let mut guard = 0usize;
        while let Some(parent) = parent_of.get(&cursor) {
            path.push(parent.clone());
            cursor = parent.clone();
            guard += 1;
            if guard > self.inventory_wizard_groups.len() {
                break;
            }
        }
        path.reverse();
        format!("all > {}", path.join(" > "))
    }

    pub fn inventory_wizard_child_groups_for_target(&self) -> Vec<String> {
        let known = self
            .inventory_wizard_groups
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        if let Some(parent) = self.inventory_wizard_target_group.as_ref() {
            return self
                .inventory_wizard_group_children
                .get(parent)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|group| known.contains(group))
                .collect::<Vec<_>>();
        }

        let mut parent_of = BTreeMap::new();
        for parent in &self.inventory_wizard_groups {
            if let Some(children) = self.inventory_wizard_group_children.get(parent) {
                for child in children {
                    if known.contains(child) {
                        parent_of
                            .entry(child.clone())
                            .or_insert_with(|| parent.clone());
                    }
                }
            }
        }
        self.inventory_wizard_groups
            .iter()
            .filter(|group| !parent_of.contains_key(*group))
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn inventory_wizard_hosts_for_target(&self) -> Vec<String> {
        if let Some(group) = self.inventory_wizard_target_group.as_ref() {
            return self
                .inventory_wizard_assignments
                .get(group)
                .cloned()
                .unwrap_or_default();
        }

        let mut assigned = HashSet::new();
        for hosts in self.inventory_wizard_assignments.values() {
            for host in hosts {
                assigned.insert(host.clone());
            }
        }
        self.inventory_wizard_hosts
            .iter()
            .filter(|host| !assigned.contains(*host))
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn inventory_wizard_selected_group_attached_to_target(&self) -> bool {
        let Some(group) = self.selected_inventory_wizard_group_candidate() else {
            return false;
        };
        if let Some(parent) = self.inventory_wizard_target_group.as_ref() {
            return self
                .inventory_wizard_group_children
                .get(parent)
                .map(|children| children.contains(&group))
                .unwrap_or(false);
        }

        !self
            .inventory_wizard_group_children
            .values()
            .any(|children| children.contains(&group))
    }

    pub fn inventory_wizard_selected_host_assigned_to_target(&self) -> bool {
        let Some(host) = self.selected_inventory_wizard_host() else {
            return false;
        };
        if let Some(group) = self.inventory_wizard_target_group.as_ref() {
            return self
                .inventory_wizard_assignments
                .get(group)
                .map(|hosts| hosts.contains(&host))
                .unwrap_or(false);
        }

        !self
            .inventory_wizard_assignments
            .values()
            .any(|hosts| hosts.contains(&host))
    }

    fn normalized_inventory_wizard_assignments(
        &self,
        groups: &[String],
        hosts: &[String],
    ) -> Option<BTreeMap<String, Vec<String>>> {
        let mut out = BTreeMap::new();
        for group in groups {
            let list = self
                .inventory_wizard_assignments
                .get(group)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|host| hosts.contains(host))
                .collect::<Vec<_>>();
            out.insert(group.clone(), list);
        }
        Some(out)
    }

    fn normalized_inventory_wizard_group_children(
        &self,
        groups: &[String],
    ) -> Option<BTreeMap<String, Vec<String>>> {
        let mut out = BTreeMap::new();
        for parent in groups {
            let children = self
                .inventory_wizard_group_children
                .get(parent)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|child| child != parent && groups.contains(child))
                .collect::<Vec<_>>();
            out.insert(parent.clone(), children);
        }
        if group_children_has_cycle(&out) {
            return None;
        }
        Some(out)
    }

    fn save_inventory_wizard(&mut self) {
        if !self.inventory_wizard_open {
            return;
        }

        let Some(filename) = normalize_inventory_filename(&self.inventory_wizard_filename) else {
            self.status_line =
                String::from("Builder: filename must be plain and end with .yml/.yaml");
            return;
        };
        if self.inventory_wizard_hosts.is_empty() {
            self.status_line = String::from("Builder: add at least one host");
            return;
        }
        if self
            .inventory_wizard_hosts
            .iter()
            .any(|host| !is_valid_inventory_key(host))
        {
            self.status_line =
                String::from("Builder: host supports letters, numbers, '.', '-', '_' and ':'");
            return;
        }
        if self
            .inventory_wizard_groups
            .iter()
            .any(|group| !is_valid_inventory_key(group) || is_reserved_inventory_group(group))
        {
            self.status_line = String::from(
                "Builder: group names must be valid and cannot be reserved (all, ungrouped)",
            );
            return;
        }

        let groups = self.inventory_wizard_groups.clone();
        let hosts = self.inventory_wizard_hosts.clone();
        let group_hosts = self
            .normalized_inventory_wizard_assignments(&groups, &hosts)
            .unwrap_or_default();
        let Some(group_children) = self.normalized_inventory_wizard_group_children(&groups) else {
            self.status_line = String::from("Builder: child-group links contain a cycle");
            return;
        };

        let inventories_dir = self.cwd.join("inventories");
        if let Err(err) = fs::create_dir_all(&inventories_dir) {
            self.status_line = format!("Builder: failed to create inventories directory: {err}");
            return;
        }

        let requested_path = inventories_dir.join(filename);
        let mut updated_existing = false;
        let output_path = if let Some(edit_path) = self.inventory_wizard_edit_path.clone() {
            if requested_path == edit_path {
                updated_existing = true;
                edit_path
            } else if requested_path.exists() {
                self.status_line = format!(
                    "Builder: inventory already exists: {}",
                    display_path(&self.cwd, &requested_path)
                );
                return;
            } else {
                requested_path
            }
        } else if requested_path.exists() {
            self.status_line = format!(
                "Builder: inventory already exists: {}",
                display_path(&self.cwd, &requested_path)
            );
            return;
        } else {
            requested_path
        };

        let content = render_inventory_yaml(&hosts, &groups, &group_hosts, &group_children);
        if let Err(err) = fs::write(&output_path, content) {
            self.status_line = format!("Builder: failed to write inventory: {err}");
            return;
        }

        self.inventory_wizard_open = false;
        self.inventory_wizard_filename.clear();
        self.inventory_wizard_edit_path = None;
        self.inventory_wizard_hosts.clear();
        self.inventory_wizard_groups.clear();
        self.inventory_wizard_assignments.clear();
        self.inventory_wizard_group_children.clear();
        self.inventory_wizard_focus = InventoryWizardFocus::Tree;
        self.inventory_wizard_target_group = None;
        self.inventory_wizard_tree_idx = 0;
        self.inventory_wizard_host_idx = 0;
        self.inventory_wizard_group_idx = 0;
        self.inventory_wizard_input_mode = None;
        self.inventory_wizard_input_buffer.clear();
        self.refresh_project();
        if let Some(idx) = self.inventories.iter().position(|p| p == &output_path) {
            self.inventory_idx = idx;
        }
        if updated_existing {
            self.status_line = format!(
                "Builder: updated inventory {}",
                display_path(&self.cwd, &output_path)
            );
        } else {
            self.status_line = format!(
                "Builder: created inventory {}",
                display_path(&self.cwd, &output_path)
            );
        }
    }

    fn open_inventory_create_prompt(&mut self) {
        if self.current_view() != View::Inventory {
            self.status_line = String::from("Inventory create is available in Inventory tab");
            return;
        }
        self.inventory_create_open = true;
        self.inventory_create_buffer.clear();
        self.pending_inventory_delete = None;
        self.status_line = String::from("New inventory: type filename and press Enter");
    }

    fn cancel_inventory_create_prompt(&mut self) {
        self.inventory_create_open = false;
        self.inventory_create_buffer.clear();
        self.status_line = String::from("Inventory create cancelled");
    }

    fn push_inventory_create_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        self.inventory_create_buffer.push(ch);
    }

    fn confirm_inventory_create(&mut self) {
        if !self.inventory_create_open {
            return;
        }
        let mut filename = self.inventory_create_buffer.trim().to_string();
        if filename.is_empty() {
            self.status_line = String::from("Inventory filename cannot be empty");
            return;
        }
        if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
            self.status_line = String::from("Inventory filename must be a plain file name");
            return;
        }

        if Path::new(&filename).extension().is_none() {
            filename.push_str(".ini");
        }
        let ext = Path::new(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        if !matches!(ext, "ini" | "yml" | "yaml") {
            self.status_line = String::from("Supported inventory extensions: .ini, .yml, .yaml");
            return;
        }

        let inventories_dir = self.cwd.join("inventories");
        if let Err(err) = fs::create_dir_all(&inventories_dir) {
            self.status_line = format!("Failed to create inventories directory: {err}");
            return;
        }
        let new_path = inventories_dir.join(&filename);
        if new_path.exists() {
            self.status_line = format!(
                "Inventory already exists: {}",
                display_path(&self.cwd, &new_path)
            );
            return;
        }

        let template = if ext == "ini" {
            "[all]\nlocalhost ansible_connection=local\n"
        } else {
            "all:\n  hosts:\n    localhost:\n      ansible_connection: local\n"
        };
        if let Err(err) = fs::write(&new_path, template) {
            self.status_line = format!("Failed to create inventory: {err}");
            return;
        }

        self.inventory_create_open = false;
        self.inventory_create_buffer.clear();
        self.pending_inventory_delete = None;
        self.refresh_project();
        if let Some(idx) = self.inventories.iter().position(|p| p == &new_path) {
            self.inventory_idx = idx;
        }
        self.status_line = format!("Created inventory {}", display_path(&self.cwd, &new_path));
    }

    fn request_inventory_delete(&mut self) {
        let Some(path) = self.inventories.get(self.inventory_idx).cloned() else {
            self.status_line = String::from("No inventory selected");
            return;
        };

        if self
            .pending_inventory_delete
            .as_ref()
            .map(|p| p == &path)
            .unwrap_or(false)
        {
            self.pending_inventory_delete = None;
            self.delete_inventory_file(path);
            return;
        }

        let display = display_path(&self.cwd, &path);
        self.pending_inventory_delete = Some(path);
        self.status_line = format!("Press Shift+D again to delete {display}");
    }

    fn delete_inventory_file(&mut self, path: PathBuf) {
        let inventories_root = self.cwd.join("inventories");
        if path.strip_prefix(&inventories_root).is_err() {
            self.status_line = String::from("Refusing to delete outside ./inventories");
            return;
        }

        match fs::remove_file(&path) {
            Ok(_) => {
                let display = display_path(&self.cwd, &path);
                self.refresh_project();
                self.status_line = format!("Deleted inventory {display}");
            }
            Err(err) => {
                self.status_line = format!("Failed to delete inventory: {err}");
            }
        }
    }

    fn restore_history(&mut self) {
        match load_runs(&self.cwd) {
            Ok(runs) => {
                if runs.is_empty() {
                    return;
                }
                self.next_run_id = runs.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                self.runs = runs;
                self.run_idx = 0;
                self.sync_run_selection_to_selected_playbook();
                self.status_line = format!("Loaded {} historical runs", self.runs.len());
            }
            Err(err) => {
                self.status_line = format!("history load failed: {err}");
            }
        }
    }

    fn persist_run(&mut self, run_id: u64) {
        let Some(run) = self.runs.iter().find(|r| r.id == run_id) else {
            return;
        };
        if let Err(err) = save_run(&self.cwd, run) {
            self.status_line = format!("history save failed: {err}");
        }
    }

    fn warn_if_playbook_bin_missing(&mut self) {
        if !playbook_bin_available(&self.run_options.ansible_bin) {
            self.status_line = format!(
                "{} not found. Press u to choose runtime or b to bootstrap managed runtime",
                self.run_options.ansible_bin
            );
            self.open_runtime_prompt();
        }
    }

    fn open_runtime_prompt(&mut self) {
        self.refresh_runtime_candidates();
        self.runtime_prompt_open = true;
        if self.runtime_candidate_idx >= self.runtime_candidates.len() {
            self.runtime_candidate_idx = self.runtime_candidates.len().saturating_sub(1);
        }
    }

    fn select_runtime_candidate(&mut self) {
        let Some(candidate) = self
            .runtime_candidates
            .get(self.runtime_candidate_idx)
            .cloned()
        else {
            self.status_line =
                String::from("No runtime candidates available. Press b to bootstrap.");
            return;
        };
        if !candidate.available {
            self.status_line = format!("Selected runtime unavailable: {}", candidate.ansible_bin);
            return;
        }

        self.run_options.ansible_bin = candidate.ansible_bin.clone();
        self.persist_runtime_config();
        self.status_line = format!("Runtime selected: {}", candidate.label);
        self.runtime_prompt_open = false;
        self.refresh_runtime_candidates();
    }

    fn refresh_runtime_candidates(&mut self) {
        self.runtime_candidates =
            discover_runtime_candidates(&self.cwd, Some(&self.run_options.ansible_bin));
        if self.runtime_candidate_idx >= self.runtime_candidates.len() {
            self.runtime_candidate_idx = self.runtime_candidates.len().saturating_sub(1);
        }
    }

    fn bootstrap_managed_runtime(&mut self, tx: &UnboundedSender<Action>) {
        if self.runtime_bootstrapping {
            self.status_line = String::from("Runtime bootstrap already running");
            return;
        }
        self.runtime_bootstrapping = true;
        self.runtime_prompt_open = true;
        self.record_runtime_log(String::from("Starting managed runtime bootstrap"));
        spawn_bootstrap_managed_runtime(self.cwd.clone(), tx.clone());
    }

    fn record_runtime_log(&mut self, line: String) {
        self.runtime_logs.push(line);
        if self.runtime_logs.len() > MAX_RUNTIME_LOG_LINES {
            let over = self
                .runtime_logs
                .len()
                .saturating_sub(MAX_RUNTIME_LOG_LINES);
            self.runtime_logs.drain(0..over);
        }
    }

    fn persist_runtime_config(&mut self) {
        self.persist_global_settings();
    }

    fn persist_global_settings(&mut self) {
        self.ansible_cfg.verbosity = self.run_options.verbosity.min(4);
        self.ansible_cfg.forks = self.run_options.forks;
        self.ansible_cfg.timeout = self.run_options.timeout;

        let config = AppConfig {
            ansible_bin: Some(self.run_options.ansible_bin.clone()),
            check: Some(self.run_options.check),
            diff: Some(self.run_options.diff),
            become_enabled: Some(self.run_options.become_enabled),
            verbosity: Some(self.run_options.verbosity.min(4)),
            forks: self.run_options.forks,
            timeout: self.run_options.timeout,
            limit: self.run_options.limit.clone(),
            tags: self.run_options.tags.clone(),
            extra_vars: self.run_options.extra_vars.clone(),
            extra_args: self.run_options.extra_args.clone(),
        };
        if let Err(err) = save_app_config(&self.cwd, &config) {
            self.status_line = format!("config save failed: {err}");
        }
        self.persist_ansible_cfg_settings();
    }

    fn restore_playbook_settings(&mut self) {
        match load_playbook_settings(&self.cwd) {
            Ok(settings) => {
                self.playbook_settings = settings;
                self.ensure_settings_for_playbooks();
            }
            Err(err) => {
                self.status_line = format!("playbook settings load failed: {err}");
            }
        }
    }

    fn persist_playbook_settings(&mut self) {
        if let Err(err) = save_playbook_settings(&self.cwd, &self.playbook_settings) {
            self.status_line = format!("playbook settings save failed: {err}");
        }
    }

    fn restore_ansible_cfg_settings(&mut self) {
        match load_ansible_cfg_settings(&self.cwd) {
            Ok(settings) => {
                self.ansible_cfg = settings;
                if std::env::var("ANSIBLE_TUI_VERBOSITY").is_err() {
                    self.run_options.verbosity = self.ansible_cfg.verbosity.min(4);
                }
                if std::env::var("ANSIBLE_TUI_FORKS").is_err() {
                    self.run_options.forks = self.ansible_cfg.forks;
                }
                if std::env::var("ANSIBLE_TUI_TIMEOUT").is_err() {
                    self.run_options.timeout = self.ansible_cfg.timeout;
                }
            }
            Err(err) => {
                self.status_line = format!("ansible.cfg load failed: {err}");
            }
        }
    }

    fn persist_ansible_cfg_settings(&mut self) {
        if let Err(err) = save_ansible_cfg_settings(&self.cwd, &self.ansible_cfg) {
            self.status_line = format!("ansible.cfg save failed: {err}");
        }
    }

    fn ensure_settings_for_playbooks(&mut self) {
        let keys = self
            .playbooks
            .iter()
            .map(|p| display_path(&self.cwd, p))
            .collect::<Vec<_>>();
        for key in &keys {
            if !self.playbook_settings.contains_key(key) {
                self.playbook_settings
                    .insert(key.clone(), self.default_settings());
            }
        }
        self.playbook_settings
            .retain(|key, _| keys.iter().any(|k| k == key));
        self.selected_run_by_playbook
            .retain(|key, _| keys.iter().any(|k| k == key));
        self.persist_playbook_settings();
    }

    fn default_settings(&self) -> PlaybookSettings {
        PlaybookSettings {
            check: self.run_options.check,
            diff: self.run_options.diff,
            become_enabled: self.run_options.become_enabled,
            verbosity: self.run_options.verbosity.min(4),
            forks: self.run_options.forks,
            timeout: self.run_options.timeout,
            limit: self.run_options.limit.clone(),
            tags: self.run_options.tags.clone(),
            extra_vars: self.run_options.extra_vars.clone(),
            extra_args: self.run_options.extra_args.clone(),
        }
    }

    fn selected_playbook_key(&self) -> Option<String> {
        self.playbooks
            .get(self.playbook_idx)
            .map(|p| display_path(&self.cwd, p))
    }

    fn open_playbook_settings(&mut self) {
        if self.settings_editor_open {
            self.close_playbook_settings();
            return;
        }
        if self.current_view() != View::Playbooks {
            self.status_line = String::from("Playbook settings are available in Playbooks tab");
            return;
        }
        let Some(key) = self.selected_playbook_key() else {
            self.status_line = String::from("No playbook selected");
            return;
        };
        let default_settings = self.default_settings();
        self.playbook_settings
            .entry(key)
            .or_insert(default_settings);
        self.settings_editor_open = true;
        self.settings_editor_field_idx = 0;
        self.settings_editor_text_mode = false;
        self.settings_editor_text_buffer.clear();
        self.status_line =
            String::from("Playbook settings: j/k field, h/l or arrows adjust, Enter edit/save");
    }

    fn close_playbook_settings(&mut self) {
        if self.settings_editor_open {
            self.settings_editor_text_mode = false;
            self.settings_editor_text_buffer.clear();
            self.settings_editor_open = false;
            self.status_line = String::from("Playbook settings closed");
        }
    }

    fn toggle_settings_boolean_field(&mut self) {
        if self.settings_editor_text_mode {
            return;
        }
        let Some(key) = self.selected_playbook_key() else {
            return;
        };
        let Some(settings) = self.playbook_settings.get_mut(&key) else {
            return;
        };

        match self.settings_editor_field_idx {
            0 => settings.check = !settings.check,
            1 => settings.diff = !settings.diff,
            2 => settings.become_enabled = !settings.become_enabled,
            _ => return,
        }
        self.persist_playbook_settings();
    }

    fn adjust_settings_field(&mut self, delta: i8) {
        if !self.settings_editor_open || self.settings_editor_text_mode {
            return;
        }
        let Some(key) = self.selected_playbook_key() else {
            return;
        };
        let Some(settings) = self.playbook_settings.get_mut(&key) else {
            return;
        };

        match self.settings_editor_field_idx {
            0 => settings.check = !settings.check,
            1 => settings.diff = !settings.diff,
            2 => settings.become_enabled = !settings.become_enabled,
            3 => {
                let v = settings.verbosity as i8 + delta;
                settings.verbosity = max(0, min(4, v)) as u8;
            }
            4 => {
                settings.forks = cycle_u16(
                    settings.forks,
                    &[None, Some(5), Some(10), Some(20), Some(50)],
                    delta,
                );
            }
            5 => {
                settings.timeout = cycle_u16(
                    settings.timeout,
                    &[None, Some(10), Some(30), Some(60), Some(120)],
                    delta,
                );
            }
            _ => {}
        }
        self.persist_playbook_settings();
    }

    pub fn selected_playbook_settings(&self) -> Option<PlaybookSettings> {
        let key = self.selected_playbook_key()?;
        self.playbook_settings.get(&key).cloned()
    }

    pub fn selected_playbook_display(&self) -> Option<String> {
        self.selected_playbook_key()
    }

    fn inventory_path_for_display(&self, inventory: &str) -> Option<PathBuf> {
        self.inventories
            .iter()
            .find(|path| display_path(&self.cwd, path) == inventory)
            .cloned()
    }

    pub fn selected_inventory_display_for_current_playbook(&self) -> Option<String> {
        let playbook = self.selected_playbook_key()?;
        if let Some(inventory) = self.selected_inventory_by_playbook.get(&playbook) {
            if self.inventory_path_for_display(inventory).is_some() {
                return Some(inventory.clone());
            }
        }
        self.inventories
            .get(self.inventory_idx)
            .map(|path| display_path(&self.cwd, path))
    }

    fn selected_inventory_path_for_current_playbook(&self) -> Option<PathBuf> {
        self.selected_inventory_display_for_current_playbook()
            .and_then(|inventory| self.inventory_path_for_display(&inventory))
    }

    fn cycle_playbook_inventory(&mut self, delta: i8) {
        if self.current_view() != View::Playbooks {
            return;
        }
        if self.inventories.is_empty() {
            self.status_line = String::from("No inventories found under ./inventories");
            return;
        }
        let Some(playbook) = self.selected_playbook_key() else {
            self.status_line = String::from("No playbook selected");
            return;
        };

        let current_display = self
            .selected_inventory_display_for_current_playbook()
            .unwrap_or_else(|| display_path(&self.cwd, &self.inventories[0]));
        let current_idx = self
            .inventories
            .iter()
            .position(|path| display_path(&self.cwd, path) == current_display)
            .unwrap_or(self.inventory_idx.min(self.inventories.len() - 1));

        let next_idx = if delta.is_positive() {
            min(current_idx + 1, self.inventories.len() - 1)
        } else {
            current_idx.saturating_sub(1)
        };
        let next_inventory = display_path(&self.cwd, &self.inventories[next_idx]);
        self.selected_inventory_by_playbook
            .insert(playbook, next_inventory.clone());
        self.status_line = format!("Playbook inventory target: {next_inventory}");
    }

    pub fn run_indices_for_selected_playbook(&self) -> Vec<usize> {
        let Some(playbook) = self.selected_playbook_display() else {
            return Vec::new();
        };
        self.runs
            .iter()
            .enumerate()
            .filter_map(|(idx, run)| {
                if run.playbook == playbook {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    fn sync_run_selection_to_selected_playbook(&mut self) {
        let Some(playbook) = self.selected_playbook_display() else {
            self.run_idx = self.runs.len();
            self.log_anchor = None;
            self.log_cursor = 0;
            return;
        };
        let indices = self
            .runs
            .iter()
            .enumerate()
            .filter_map(|(idx, run)| {
                if run.playbook == playbook {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if indices.is_empty() {
            self.run_idx = self.runs.len();
            self.log_anchor = None;
            self.log_cursor = 0;
            return;
        }
        let remembered_idx = self
            .selected_run_by_playbook
            .get(&playbook)
            .and_then(|run_id| {
                indices
                    .iter()
                    .copied()
                    .find(|idx| self.runs[*idx].id == *run_id)
            });
        let next_idx = remembered_idx.or_else(|| {
            if indices.contains(&self.run_idx) {
                Some(self.run_idx)
            } else {
                indices.first().copied()
            }
        });
        if let Some(idx) = next_idx {
            if self.run_idx != idx {
                self.run_idx = idx;
                self.log_anchor = None;
                self.sync_log_cursor_to_selected_run();
            }
            self.remember_selected_run_for_current_playbook();
        }
    }

    fn select_playbook_run_offset(&mut self, delta: isize) {
        if self.current_view() != View::Playbooks {
            return;
        }
        self.playbooks_focus_runs = true;
        self.move_selected_playbook_run(delta, true);
    }

    fn move_selected_playbook_run(&mut self, delta: isize, show_empty_status: bool) {
        let indices = self.run_indices_for_selected_playbook();
        if indices.is_empty() {
            if show_empty_status {
                self.status_line = String::from("No runs for selected playbook yet");
            }
            return;
        }
        let current_pos = indices
            .iter()
            .position(|idx| *idx == self.run_idx)
            .unwrap_or(0);
        let next = (current_pos as isize + delta).clamp(0, indices.len().saturating_sub(1) as isize)
            as usize;
        self.run_idx = indices[next];
        self.log_anchor = None;
        self.sync_log_cursor_to_selected_run();
        self.remember_selected_run_for_current_playbook();
        if let Some(run) = self.runs.get(self.run_idx) {
            self.status_line = format!(
                "Viewing logs for run #{:03} ({})",
                run.id,
                run.status.as_str()
            );
        }
    }

    fn remember_selected_run_for_current_playbook(&mut self) {
        let Some(playbook) = self.selected_playbook_key() else {
            return;
        };
        let Some(run) = self.runs.get(self.run_idx) else {
            return;
        };
        if run.playbook == playbook {
            self.selected_run_by_playbook.insert(playbook, run.id);
        }
    }

    fn sync_log_cursor_to_selected_run(&mut self) {
        self.log_cursor = self
            .runs
            .get(self.run_idx)
            .map(|run| run.logs.len().saturating_sub(1))
            .unwrap_or(0);
    }

    fn log_index_from_view_row(&self, row: u16, viewport_height: u16) -> Option<usize> {
        let run = self.runs.get(self.run_idx)?;
        if run.logs.is_empty() {
            return None;
        }

        let viewport_height = viewport_height as usize;
        let line_prefix_rows = 2; // summary + blank
        if (row as usize) < line_prefix_rows {
            return None;
        }

        let log_slots = viewport_height.saturating_sub(line_prefix_rows);
        if log_slots == 0 {
            return None;
        }

        let start = if self.log_select_mode {
            let cursor = self.log_cursor.min(run.logs.len() - 1);
            let mut start = cursor.saturating_sub(log_slots.saturating_sub(1));
            if start + log_slots > run.logs.len() {
                start = run.logs.len().saturating_sub(log_slots);
            }
            start
        } else {
            run.logs.len().saturating_sub(log_slots)
        };

        let rel = row as usize - line_prefix_rows;
        let idx = start + rel;
        if idx < run.logs.len() {
            Some(idx)
        } else {
            None
        }
    }
}

fn normalize_optional_text(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub fn display_path(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

struct ParsedInventoryYaml {
    hosts: Vec<String>,
    groups: Vec<String>,
    assignments: BTreeMap<String, Vec<String>>,
    group_children: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedInventoryGroupSection {
    None,
    Hosts,
    Children,
}

fn parse_yaml_mapping_key(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let key = if let Some((left, _right)) = trimmed.split_once(':') {
        left.trim()
    } else {
        return None;
    };
    if key.is_empty() {
        return None;
    }
    Some(key.to_string())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn parse_inventory_yaml_for_builder(content: &str) -> Result<ParsedInventoryYaml, String> {
    let mut found_all_root = false;
    let mut in_all_hosts = false;
    let mut in_children = false;
    let mut current_group: Option<String> = None;
    let mut current_group_section = ParsedInventoryGroupSection::None;

    let mut hosts = Vec::new();
    let mut groups = Vec::new();
    let mut assignments: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut group_children: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.chars().take_while(|c| *c == ' ').count();

        if indent == 0 {
            found_all_root = trimmed == "all:";
            in_all_hosts = false;
            in_children = false;
            current_group = None;
            current_group_section = ParsedInventoryGroupSection::None;
            continue;
        }
        if !found_all_root {
            continue;
        }

        match indent {
            2 => {
                in_all_hosts = trimmed == "hosts:";
                in_children = trimmed == "children:";
                current_group = None;
                current_group_section = ParsedInventoryGroupSection::None;
            }
            4 => {
                if in_all_hosts {
                    if let Some(host) = parse_yaml_mapping_key(trimmed) {
                        if host != "hosts" {
                            push_unique(&mut hosts, host);
                        }
                    }
                } else if in_children {
                    if let Some(group) = parse_yaml_mapping_key(trimmed) {
                        if group != "hosts" && group != "children" {
                            push_unique(&mut groups, group.clone());
                            assignments.entry(group.clone()).or_default();
                            group_children.entry(group.clone()).or_default();
                            current_group = Some(group);
                            current_group_section = ParsedInventoryGroupSection::None;
                        }
                    }
                }
            }
            6 => {
                if in_children && current_group.is_some() {
                    current_group_section = match trimmed {
                        "hosts:" => ParsedInventoryGroupSection::Hosts,
                        "children:" => ParsedInventoryGroupSection::Children,
                        "hosts: {}" | "children: {}" => ParsedInventoryGroupSection::None,
                        _ => current_group_section,
                    };
                }
            }
            8 => {
                let Some(group) = current_group.clone() else {
                    continue;
                };
                let Some(item) = parse_yaml_mapping_key(trimmed) else {
                    continue;
                };
                match current_group_section {
                    ParsedInventoryGroupSection::Hosts => {
                        push_unique(&mut hosts, item.clone());
                        let entry = assignments.entry(group).or_default();
                        if !entry.contains(&item) {
                            entry.push(item);
                        }
                    }
                    ParsedInventoryGroupSection::Children => {
                        if item == group {
                            continue;
                        }
                        push_unique(&mut groups, item.clone());
                        assignments.entry(item.clone()).or_default();
                        group_children.entry(item.clone()).or_default();
                        let entry = group_children.entry(group).or_default();
                        if !entry.contains(&item) {
                            entry.push(item);
                        }
                    }
                    ParsedInventoryGroupSection::None => {}
                }
            }
            _ => {}
        }
    }

    if !found_all_root {
        return Err(String::from("missing top-level 'all:' root"));
    }

    for group in groups.clone() {
        assignments.entry(group.clone()).or_default();
        group_children.entry(group).or_default();
    }

    if group_children_has_cycle(&group_children) {
        return Err(String::from("group relationships contain a cycle"));
    }

    Ok(ParsedInventoryYaml {
        hosts,
        groups,
        assignments,
        group_children,
    })
}

fn discover_project(cwd: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut inventories = Vec::new();
    let mut playbooks = Vec::new();

    for entry in WalkDir::new(cwd)
        .follow_links(false)
        .max_depth(4)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if path.components().any(|component| {
            let dir = component.as_os_str();
            dir == "target" || dir == ".ansible-tui"
        }) {
            continue;
        }

        let parent = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|p| p.to_str());
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();

        if parent == Some("inventories") && is_inventory_ext(ext) {
            inventories.push(path.to_path_buf());
            continue;
        }

        let in_playbooks_dir = parent == Some("playbooks");
        let in_project_root = path.parent().map(|p| p == cwd).unwrap_or(false);
        if (in_playbooks_dir || in_project_root) && is_playbook_ext(ext) {
            playbooks.push(path.to_path_buf());
        }
    }

    inventories.sort();
    inventories.dedup();
    playbooks.sort();
    playbooks.dedup();

    (inventories, playbooks)
}

fn is_inventory_ext(ext: &str) -> bool {
    matches!(ext, "yml" | "yaml" | "ini")
}

fn is_playbook_ext(ext: &str) -> bool {
    matches!(ext, "yml" | "yaml")
}

fn normalize_inventory_filename(value: &str) -> Option<String> {
    let mut filename = value.trim().to_string();
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
    {
        return None;
    }
    if Path::new(&filename).extension().is_none() {
        filename.push_str(".yml");
    }
    let ext = Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if !matches!(ext, "yml" | "yaml") {
        return None;
    }
    Some(filename)
}

fn is_valid_inventory_key(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | ':'))
}

fn is_reserved_inventory_group(group: &str) -> bool {
    matches!(group, "all" | "ungrouped")
}

fn shell_escape_single_quoted(value: &str) -> String {
    value.replace('\'', "'\\''")
}

fn suspend_tui_for_external_process() -> Result<(), String> {
    disable_raw_mode().map_err(|err| err.to_string())?;
    let mut stdout = std::io::stdout();
    execute!(stdout, DisableMouseCapture).map_err(|err| err.to_string())?;
    Ok(())
}

fn resume_tui_after_external_process() -> Result<(), String> {
    enable_raw_mode().map_err(|err| err.to_string())?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        EnableMouseCapture,
        Clear(ClearType::All),
        MoveTo(0, 0)
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn drain_stale_terminal_input(window: Duration) {
    let deadline = Instant::now() + window;
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let wait = (deadline - now).min(Duration::from_millis(10));
        match event::poll(wait) {
            Ok(true) => {
                let _ = event::read();
            }
            Ok(false) => {}
            Err(_) => break,
        }
    }

    while matches!(event::poll(Duration::from_millis(0)), Ok(true)) {
        let _ = event::read();
    }
}

fn render_inventory_yaml(
    hosts: &[String],
    groups: &[String],
    group_hosts: &BTreeMap<String, Vec<String>>,
    group_children: &BTreeMap<String, Vec<String>>,
) -> String {
    let mut out = String::from("all:\n");
    let mut assigned_hosts = Vec::new();
    for host in group_hosts.values().flatten() {
        if !assigned_hosts.contains(host) {
            assigned_hosts.push(host.clone());
        }
    }
    let unassigned_hosts = hosts
        .iter()
        .filter(|host| !assigned_hosts.contains(*host))
        .collect::<Vec<_>>();

    if !unassigned_hosts.is_empty() {
        out.push_str("  hosts:\n");
        for host in unassigned_hosts {
            out.push_str(&format!("    {host}: {{}}\n"));
        }
    }

    out.push_str("  children:\n");
    for group in groups {
        out.push_str(&format!("    {group}:\n"));
        if let Some(hosts) = group_hosts.get(group) {
            if hosts.is_empty() {
                out.push_str("      hosts: {}\n");
            } else {
                out.push_str("      hosts:\n");
                for host in hosts {
                    out.push_str(&format!("        {host}: {{}}\n"));
                }
            }
        } else {
            out.push_str("      hosts: {}\n");
        }

        if let Some(children) = group_children.get(group) {
            if !children.is_empty() {
                out.push_str("      children:\n");
                for child in children {
                    out.push_str(&format!("        {child}: {{}}\n"));
                }
            }
        }
    }
    out
}

fn group_children_has_cycle(group_children: &BTreeMap<String, Vec<String>>) -> bool {
    fn dfs(
        node: &str,
        group_children: &BTreeMap<String, Vec<String>>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> bool {
        if visited.contains(node) {
            return false;
        }
        if !visiting.insert(node.to_string()) {
            return true;
        }
        if let Some(children) = group_children.get(node) {
            for child in children {
                if dfs(child, group_children, visiting, visited) {
                    return true;
                }
            }
        }
        visiting.remove(node);
        visited.insert(node.to_string());
        false
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for node in group_children.keys() {
        if dfs(node, group_children, &mut visiting, &mut visited) {
            return true;
        }
    }
    false
}

fn copy_text_to_clipboard(text: &str) -> Result<&'static str, String> {
    if copy_with_command("wl-copy", &[], text).is_ok() {
        return Ok("wl-copy");
    }
    if copy_with_command("xclip", &["-selection", "clipboard"], text).is_ok() {
        return Ok("xclip");
    }
    if copy_with_command("xsel", &["--clipboard", "--input"], text).is_ok() {
        return Ok("xsel");
    }
    if copy_with_command("pbcopy", &[], text).is_ok() {
        return Ok("pbcopy");
    }

    copy_text_via_osc52(text).map(|_| "OSC52")
}

fn copy_with_command(cmd: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{cmd} exited with {:?}", status.code()))
    }
}

fn copy_text_via_osc52(text: &str) -> Result<(), String> {
    let encoded = base64_encode(text.as_bytes());
    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]52;c;{}\x07", encoded).map_err(|e| e.to_string())?;
    stdout.flush().map_err(|e| e.to_string())
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;
        let c0 = TABLE[((n >> 18) & 0x3f) as usize] as char;
        let c1 = TABLE[((n >> 12) & 0x3f) as usize] as char;
        let c2 = TABLE[((n >> 6) & 0x3f) as usize] as char;
        let c3 = TABLE[(n & 0x3f) as usize] as char;

        out.push(c0);
        out.push(c1);
        if i + 1 < input.len() {
            out.push(c2);
        } else {
            out.push('=');
        }
        if i + 2 < input.len() {
            out.push(c3);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

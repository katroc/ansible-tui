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
use crate::job_template::{load_job_templates, save_job_templates, JobTemplate};
use crate::playbook_settings::{
    cycle_u16, load_playbook_settings, save_playbook_settings, PlaybookSettings,
};
use crate::projects::{
    default_project, load_projects, save_projects, ProjectDefinition, ProjectRegistry,
};
use crate::run::{
    discover_runtime_candidates, playbook_bin_available, spawn_ansible_run,
    spawn_bootstrap_managed_runtime, spawn_git_clone, spawn_project_sync, RunOptions, RunRequest,
    RuntimeCandidate,
};
use crate::run_store::{load_runs, save_run};

const MAX_LOG_LINES: usize = 1_000;
const MAX_RUNTIME_LOG_LINES: usize = 120;
const AUTO_DISCOVERY_INTERVAL: Duration = Duration::from_secs(2);
const PLAYBOOK_SETTINGS_FIELD_COUNT: usize = 12;
const PLAYBOOK_SETTINGS_TEXT_FIELD_START: usize = 6;
const GLOBAL_SETTINGS_FIELD_COUNT: usize = 12;
const TEMPLATE_EDITOR_FIELD_COUNT: usize = 16;
const MAX_PROJECT_SYNC_LOG_LINES: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Projects,
    Inventory,
    Playbooks,
    Templates,
    Settings,
}

impl View {
    pub fn all() -> [View; 6] {
        [
            View::Dashboard,
            View::Projects,
            View::Inventory,
            View::Playbooks,
            View::Templates,
            View::Settings,
        ]
    }

    pub fn title(self) -> &'static str {
        match self {
            View::Dashboard => "Dashboard",
            View::Projects => "Projects",
            View::Inventory => "Inventory",
            View::Playbooks => "Playbooks",
            View::Templates => "Templates",
            View::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectSyncKind {
    Inventory,
    Vars,
}

impl ProjectSyncKind {
    fn title(self) -> &'static str {
        match self {
            ProjectSyncKind::Inventory => "Inventory",
            ProjectSyncKind::Vars => "Vars",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCreateMode {
    New,
    ExistingFs,
    Git,
}

impl ProjectCreateMode {
    pub fn title(self) -> &'static str {
        match self {
            ProjectCreateMode::New => "New Project",
            ProjectCreateMode::ExistingFs => "Import Existing Project",
            ProjectCreateMode::Git => "Clone Git Project",
        }
    }
}

#[derive(Debug, Clone)]
struct PendingGitProject {
    name: String,
    root: PathBuf,
    inventory_sync_cmd: Option<String>,
    vars_sync_cmd: Option<String>,
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
pub enum GroupsFocus {
    Tree,
    Groups,
    Hosts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventorySubTab {
    Files,
    Hosts,
    Groups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum HostDetailField {
    AnsibleHost,
    AnsibleUser,
    AnsiblePort,
    AnsibleConnection,
    CustomVar(usize),
}

#[derive(Debug, Clone, Default)]
pub struct HostVars {
    pub ansible_host: String,
    pub ansible_user: String,
    pub ansible_port: Option<u16>,
    pub ansible_connection: String,
    pub custom_vars: Vec<(String, String)>,
}

impl HostVars {
    pub fn is_empty(&self) -> bool {
        self.ansible_host.is_empty()
            && self.ansible_user.is_empty()
            && self.ansible_port.is_none()
            && self.ansible_connection.is_empty()
            && self.custom_vars.is_empty()
    }

    pub fn to_yaml_mapping(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        if !self.ansible_host.is_empty() {
            map.insert("ansible_host".to_string(), self.ansible_host.clone());
        }
        if !self.ansible_user.is_empty() {
            map.insert("ansible_user".to_string(), self.ansible_user.clone());
        }
        if let Some(port) = self.ansible_port {
            map.insert("ansible_port".to_string(), port.to_string());
        }
        if !self.ansible_connection.is_empty() {
            map.insert(
                "ansible_connection".to_string(),
                self.ansible_connection.clone(),
            );
        }
        for (key, value) in &self.custom_vars {
            map.insert(key.clone(), value.clone());
        }
        map
    }
}

pub struct InventoryEditState {
    pub path: PathBuf,
    pub hosts: Vec<String>,
    pub host_vars: BTreeMap<String, HostVars>,
    pub groups: Vec<String>,
    pub assignments: BTreeMap<String, Vec<String>>,
    pub group_children: BTreeMap<String, Vec<String>>,
    pub dirty: bool,
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
    pub template_id: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TemplateEffectiveContext {
    pub inventory: String,
    pub inventory_source: String,
    pub vars_files: Vec<String>,
    pub ssh_private_key_file: Option<String>,
    pub has_inline_ssh_key: bool,
    pub ssh_key_source: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct ResolvedTemplateRunContext {
    inventory: String,
    options: RunOptions,
    inventory_source: String,
    ssh_key_source: String,
    warnings: Vec<String>,
}

pub struct App {
    pub cwd: PathBuf,
    pub projects: Vec<ProjectDefinition>,
    pub project_idx: usize,
    pub active_project_idx: usize,
    pub inventories: Vec<PathBuf>,
    pub playbooks: Vec<PathBuf>,
    pub vars_files: Vec<PathBuf>,
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
    pub inventory_sub_tab: InventorySubTab,
    pub inventory_edit_state: Option<InventoryEditState>,
    pub hosts_subtab_idx: usize,
    pub hosts_subtab_focus_detail: bool,
    pub hosts_subtab_field_idx: usize,
    pub hosts_subtab_editing: bool,
    pub hosts_subtab_edit_buffer: String,
    pub hosts_subtab_add_host_open: bool,
    pub hosts_subtab_add_host_buffer: String,
    pub hosts_subtab_add_var_open: bool,
    pub hosts_subtab_add_var_buffer: String,
    pub groups_subtab_focus: GroupsFocus,
    pub groups_subtab_tree_idx: usize,
    pub groups_subtab_group_idx: usize,
    pub groups_subtab_host_idx: usize,
    pub groups_subtab_target_group: Option<String>,
    pub project_create_open: bool,
    pub project_create_mode: ProjectCreateMode,
    pub project_create_field_idx: usize,
    pub project_create_buffer_name: String,
    pub project_create_buffer_git_url: String,
    pub project_create_buffer_root: String,
    pub project_create_buffer_inventory_sync: String,
    pub project_create_buffer_vars_sync: String,
    pub project_ssh_open: bool,
    pub project_ssh_field_idx: usize,
    pub project_ssh_buffer_file: String,
    pub project_ssh_buffer_inline: String,
    pub project_sync_running: bool,
    pub project_sync_logs: Vec<String>,
    project_ssh_target_root: Option<PathBuf>,
    pending_git_project: Option<PendingGitProject>,
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
    // Template state
    pub job_templates: Vec<JobTemplate>,
    pub template_idx: usize,
    pub templates_focus_runs: bool,
    pending_template_delete: Option<String>,

    // Template editor state
    pub template_editor_open: bool,
    pub template_editor_editing_id: Option<String>,
    pub template_editor_field_idx: usize,
    pub template_editor_text_mode: bool,
    pub template_editor_text_buffer: String,
    pub template_editor_name: String,
    pub template_editor_playbook_idx: usize,
    pub template_editor_inventory_idx: usize,
    pub template_editor_settings: PlaybookSettings,

    pub should_quit: bool,
    needs_full_redraw: bool,
    pending_project_delete: Option<PathBuf>,
    pending_inventory_delete: Option<PathBuf>,
    last_auto_discovery_at: Instant,
    next_run_id: u64,
}

impl App {
    pub fn new(cwd: PathBuf) -> Self {
        let mut run_options = RunOptions::from_env();
        let mut status_line = String::from("Ready. Press r to run selected template.");
        match load_app_config(&cwd) {
            Ok(config) => Self::apply_loaded_global_config(&mut run_options, config),
            Err(err) => {
                status_line = format!("config load failed: {err}");
            }
        }

        let mut app = Self {
            cwd,
            projects: Vec::new(),
            project_idx: 0,
            active_project_idx: 0,
            inventories: Vec::new(),
            playbooks: Vec::new(),
            vars_files: Vec::new(),
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
            inventory_sub_tab: InventorySubTab::Files,
            inventory_edit_state: None,
            hosts_subtab_idx: 0,
            hosts_subtab_focus_detail: false,
            hosts_subtab_field_idx: 0,
            hosts_subtab_editing: false,
            hosts_subtab_edit_buffer: String::new(),
            hosts_subtab_add_host_open: false,
            hosts_subtab_add_host_buffer: String::new(),
            hosts_subtab_add_var_open: false,
            hosts_subtab_add_var_buffer: String::new(),
            groups_subtab_focus: GroupsFocus::Tree,
            groups_subtab_tree_idx: 0,
            groups_subtab_group_idx: 0,
            groups_subtab_host_idx: 0,
            groups_subtab_target_group: None,
            project_create_open: false,
            project_create_mode: ProjectCreateMode::New,
            project_create_field_idx: 0,
            project_create_buffer_name: String::new(),
            project_create_buffer_git_url: String::new(),
            project_create_buffer_root: String::new(),
            project_create_buffer_inventory_sync: String::new(),
            project_create_buffer_vars_sync: String::new(),
            project_ssh_open: false,
            project_ssh_field_idx: 0,
            project_ssh_buffer_file: String::new(),
            project_ssh_buffer_inline: String::new(),
            project_sync_running: false,
            project_sync_logs: Vec::new(),
            project_ssh_target_root: None,
            pending_git_project: None,
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
            job_templates: Vec::new(),
            template_idx: 0,
            templates_focus_runs: false,
            pending_template_delete: None,
            template_editor_open: false,
            template_editor_editing_id: None,
            template_editor_field_idx: 0,
            template_editor_text_mode: false,
            template_editor_text_buffer: String::new(),
            template_editor_name: String::new(),
            template_editor_playbook_idx: 0,
            template_editor_inventory_idx: 0,
            template_editor_settings: PlaybookSettings::default(),
            should_quit: false,
            needs_full_redraw: false,
            pending_project_delete: None,
            pending_inventory_delete: None,
            last_auto_discovery_at: Instant::now(),
            next_run_id: 1,
        };
        app.restore_projects();
        app.load_active_project_state();
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

    pub fn active_project_root(&self) -> &Path {
        self.projects
            .get(self.active_project_idx)
            .map(|project| project.root.as_path())
            .unwrap_or(self.cwd.as_path())
    }

    pub fn active_project_name(&self) -> String {
        self.projects
            .get(self.active_project_idx)
            .map(|project| project.name.clone())
            .unwrap_or_else(|| String::from("Local"))
    }

    pub fn selected_project(&self) -> Option<&ProjectDefinition> {
        self.projects.get(self.project_idx)
    }

    pub fn project_ssh_target_name(&self) -> Option<String> {
        let target = self.project_ssh_target_root.as_ref()?;
        self.projects
            .iter()
            .find(|project| &project.root == target)
            .map(|project| project.name.clone())
    }

    pub fn settings_text_mode_is_multiline(&self) -> bool {
        self.settings_editor_text_mode && self.settings_editor_field_idx == 11
    }

    fn restore_projects(&mut self) {
        match load_projects(&self.cwd) {
            Ok(registry) => {
                self.projects = registry.projects;
                self.active_project_idx = registry
                    .active_idx
                    .min(self.projects.len().saturating_sub(1));
                self.project_idx = self.active_project_idx;
            }
            Err(err) => {
                self.projects = vec![default_project(&self.cwd)];
                self.active_project_idx = 0;
                self.project_idx = 0;
                self.status_line = format!("projects load failed: {err}");
            }
        }
    }

    fn persist_projects(&mut self) {
        let registry = ProjectRegistry {
            projects: self.projects.clone(),
            active_idx: self
                .active_project_idx
                .min(self.projects.len().saturating_sub(1)),
        };
        if let Err(err) = save_projects(&self.cwd, &registry) {
            self.status_line = format!("projects save failed: {err}");
        }
    }

    fn load_active_project_state(&mut self) {
        self.playbooks_focus_runs = false;
        self.templates_focus_runs = false;
        self.log_select_mode = false;
        self.log_anchor = None;
        self.log_cursor = 0;
        self.pending_project_delete = None;
        self.pending_inventory_delete = None;
        self.pending_template_delete = None;
        self.template_idx = 0;
        self.template_editor_open = false;
        self.project_ssh_open = false;
        self.project_ssh_field_idx = 0;
        self.project_ssh_buffer_file.clear();
        self.project_ssh_buffer_inline.clear();
        self.project_ssh_target_root = None;
        self.inventory_idx = 0;
        self.playbook_idx = 0;
        self.run_idx = 0;
        self.project_sync_running = false;
        self.pending_git_project = None;
        self.selected_run_by_playbook.clear();
        self.selected_inventory_by_playbook.clear();
        self.project_sync_logs.clear();
        self.refresh_project();
        self.restore_playbook_settings();
        self.restore_ansible_cfg_settings();
        self.restore_history();
        self.restore_job_templates();
        self.refresh_runtime_candidates();
    }

    fn activate_project_idx(&mut self, idx: usize) {
        if idx >= self.projects.len() {
            return;
        }
        self.active_project_idx = idx;
        self.project_idx = idx;
        self.load_active_project_state();
        self.persist_projects();
        let root = display_path(&self.cwd, self.active_project_root());
        self.status_line = format!(
            "Active project: {} ({})",
            self.active_project_name(),
            if root.is_empty() { "." } else { &root }
        );
    }

    pub fn update(&mut self, action: Action, tx: &UnboundedSender<Action>) {
        match action {
            Action::Tick => self.auto_refresh_project(),
            Action::Quit => self.request_quit(),
            Action::CharInput(ch) => self.handle_char_input(ch, tx),
            Action::Backspace => self.handle_backspace(),
            Action::NextView => {
                if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(1);
                } else if !self.settings_editor_open
                    && !self.template_editor_open
                    && !self.inventory_create_open
                    && !self.project_create_open
                    && !self.project_ssh_open
                    && !self.inventory_editor_open
                    && !self.inventory_edit_mode_open
                    && !self.runtime_prompt_open
                    && !(self.current_view() == View::Settings && self.global_settings_text_mode)
                {
                    self.view_idx = (self.view_idx + 1) % View::all().len();
                    if self.current_view() == View::Playbooks {
                        self.playbooks_focus_runs = false;
                        self.sync_run_selection_to_selected_playbook();
                    } else if self.current_view() == View::Templates {
                        self.templates_focus_runs = false;
                        self.sync_run_selection_to_selected_template();
                    }
                }
            }
            Action::PrevView => {
                if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(-1);
                } else if !self.settings_editor_open
                    && !self.template_editor_open
                    && !self.inventory_create_open
                    && !self.project_create_open
                    && !self.project_ssh_open
                    && !self.inventory_editor_open
                    && !self.inventory_edit_mode_open
                    && !self.runtime_prompt_open
                    && !(self.current_view() == View::Settings && self.global_settings_text_mode)
                {
                    self.view_idx = (self.view_idx + View::all().len() - 1) % View::all().len();
                    if self.current_view() == View::Playbooks {
                        self.playbooks_focus_runs = false;
                        self.sync_run_selection_to_selected_playbook();
                    } else if self.current_view() == View::Templates {
                        self.templates_focus_runs = false;
                        self.sync_run_selection_to_selected_template();
                    }
                }
            }
            Action::SettingsIncrease => {
                if self.template_editor_open {
                    self.adjust_template_editor_field(1);
                } else if self.settings_editor_open {
                    self.adjust_settings_field(1);
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(1);
                } else if self.current_view() == View::Inventory
                    && matches!(
                        self.inventory_sub_tab,
                        InventorySubTab::Hosts | InventorySubTab::Groups
                    )
                {
                    match self.inventory_sub_tab {
                        InventorySubTab::Hosts => self.handle_hosts_subtab_char('l'),
                        InventorySubTab::Groups => self.handle_groups_subtab_char('l'),
                        _ => {}
                    }
                } else if self.current_view() == View::Playbooks && !self.runtime_prompt_open {
                    self.playbooks_focus_runs = true;
                } else if self.current_view() == View::Templates && !self.runtime_prompt_open {
                    self.templates_focus_runs = true;
                    self.sync_run_selection_to_selected_template();
                } else {
                    self.adjust_global_settings_field(1);
                }
            }
            Action::SettingsDecrease => {
                if self.template_editor_open {
                    self.adjust_template_editor_field(-1);
                } else if self.settings_editor_open {
                    self.adjust_settings_field(-1);
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(-1);
                } else if self.current_view() == View::Inventory
                    && matches!(
                        self.inventory_sub_tab,
                        InventorySubTab::Hosts | InventorySubTab::Groups
                    )
                {
                    match self.inventory_sub_tab {
                        InventorySubTab::Hosts => self.handle_hosts_subtab_char('h'),
                        InventorySubTab::Groups => self.handle_groups_subtab_char('h'),
                        _ => {}
                    }
                } else if self.current_view() == View::Playbooks && !self.runtime_prompt_open {
                    self.playbooks_focus_runs = false;
                } else if self.current_view() == View::Templates && !self.runtime_prompt_open {
                    self.templates_focus_runs = false;
                } else {
                    self.adjust_global_settings_field(-1);
                }
            }
            Action::MoveUp => {
                if self.template_editor_open {
                    if !self.template_editor_text_mode {
                        self.template_editor_field_idx =
                            self.template_editor_field_idx.saturating_sub(1);
                    }
                } else if self.settings_editor_open {
                    if !self.settings_editor_text_mode {
                        self.settings_editor_field_idx =
                            self.settings_editor_field_idx.saturating_sub(1);
                    }
                } else if self.project_ssh_open {
                    self.project_ssh_field_idx = self.project_ssh_field_idx.saturating_sub(1);
                } else if self.project_create_open {
                    self.project_create_field_idx = self.project_create_field_idx.saturating_sub(1);
                } else if self.inventory_create_open {
                } else if self.inventory_editor_open {
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(-1);
                } else if self.runtime_prompt_open {
                    self.runtime_candidate_idx = self.runtime_candidate_idx.saturating_sub(1);
                } else if self.current_view() == View::Inventory
                    && matches!(
                        self.inventory_sub_tab,
                        InventorySubTab::Hosts | InventorySubTab::Groups
                    )
                {
                    match self.inventory_sub_tab {
                        InventorySubTab::Hosts => self.handle_hosts_subtab_char('k'),
                        InventorySubTab::Groups => self.handle_groups_subtab_char('k'),
                        _ => {}
                    }
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
                if self.template_editor_open {
                    if !self.template_editor_text_mode {
                        self.template_editor_field_idx = min(
                            self.template_editor_field_idx + 1,
                            TEMPLATE_EDITOR_FIELD_COUNT - 1,
                        );
                    }
                } else if self.settings_editor_open {
                    if !self.settings_editor_text_mode {
                        self.settings_editor_field_idx = min(
                            self.settings_editor_field_idx + 1,
                            PLAYBOOK_SETTINGS_FIELD_COUNT - 1,
                        );
                    }
                } else if self.project_ssh_open {
                    self.project_ssh_field_idx = min(self.project_ssh_field_idx + 1, 1);
                } else if self.project_create_open {
                    self.project_create_field_idx = min(
                        self.project_create_field_idx + 1,
                        self.project_create_last_field_idx(),
                    );
                } else if self.inventory_create_open {
                } else if self.inventory_editor_open {
                } else if self.inventory_edit_mode_open {
                    self.move_inventory_edit_mode_selection(1);
                } else if self.runtime_prompt_open {
                    if !self.runtime_candidates.is_empty() {
                        self.runtime_candidate_idx = min(
                            self.runtime_candidate_idx + 1,
                            self.runtime_candidates.len() - 1,
                        );
                    }
                } else if self.current_view() == View::Inventory
                    && matches!(
                        self.inventory_sub_tab,
                        InventorySubTab::Hosts | InventorySubTab::Groups
                    )
                {
                    match self.inventory_sub_tab {
                        InventorySubTab::Hosts => self.handle_hosts_subtab_char('j'),
                        InventorySubTab::Groups => self.handle_groups_subtab_char('j'),
                        _ => {}
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
            Action::LogScrollUp => self.scroll_logs_by(-1),
            Action::LogScrollDown => self.scroll_logs_by(1),
            Action::LogScrollPageUp => self.scroll_logs_by(-20),
            Action::LogScrollPageDown => self.scroll_logs_by(20),
            Action::LogFollowLatest => self.follow_logs_latest(),
            Action::ToggleCheckMode => self.toggle_check_mode(),
            Action::ToggleDiffMode => self.toggle_diff_mode(),
            Action::SaveInventoryEditor => {
                if self.project_ssh_open {
                    self.save_project_ssh_prompt();
                } else if self.template_editor_open {
                    if self.template_editor_text_mode {
                        self.commit_template_editor_text_edit();
                    } else {
                        self.save_template_from_editor();
                    }
                } else if self.settings_editor_open && self.settings_editor_text_mode {
                    self.commit_settings_text_edit();
                } else if self.current_view() == View::Inventory
                    && matches!(
                        self.inventory_sub_tab,
                        InventorySubTab::Hosts | InventorySubTab::Groups
                    )
                {
                    self.save_inventory_edit_state();
                } else {
                    self.save_inventory_editor();
                }
            }
            Action::OpenRuntimePrompt => self.open_runtime_prompt(),
            Action::CloseRuntimePrompt => {
                if self.template_editor_open {
                    if self.template_editor_text_mode {
                        self.cancel_template_editor_text_edit();
                    } else {
                        self.close_template_editor();
                    }
                } else if self.settings_editor_open && self.settings_editor_text_mode {
                    self.cancel_settings_text_edit();
                } else if self.settings_editor_open {
                    self.close_playbook_settings();
                } else if self.inventory_create_open {
                    self.cancel_inventory_create_prompt();
                } else if self.project_create_open {
                    self.cancel_project_create_prompt();
                } else if self.project_ssh_open {
                    self.cancel_project_ssh_prompt();
                } else if self.inventory_editor_open {
                    self.close_inventory_editor(false);
                } else if self.inventory_edit_mode_open {
                    self.close_inventory_edit_mode_prompt();
                } else if self.current_view() == View::Inventory
                    && matches!(
                        self.inventory_sub_tab,
                        InventorySubTab::Hosts | InventorySubTab::Groups
                    )
                {
                    if self.hosts_subtab_editing {
                        self.hosts_subtab_editing = false;
                        self.hosts_subtab_edit_buffer.clear();
                    } else if self.hosts_subtab_add_host_open {
                        self.hosts_subtab_add_host_open = false;
                        self.hosts_subtab_add_host_buffer.clear();
                    } else if self.hosts_subtab_add_var_open {
                        self.hosts_subtab_add_var_open = false;
                        self.hosts_subtab_add_var_buffer.clear();
                    } else {
                        self.inventory_sub_tab = InventorySubTab::Files;
                    }
                } else if self.current_view() == View::Settings && self.global_settings_text_mode {
                    self.cancel_global_settings_text_edit();
                } else {
                    self.runtime_prompt_open = false;
                }
            }
            Action::SelectRuntimeCandidate => {
                if self.template_editor_open {
                    self.confirm_template_editor();
                } else if self.settings_editor_open {
                    self.confirm_settings_editor();
                } else if self.inventory_editor_open {
                    self.insert_inventory_editor_newline();
                } else if self.inventory_create_open {
                    self.confirm_inventory_create();
                } else if self.project_create_open {
                    self.confirm_project_create(tx);
                } else if self.project_ssh_open {
                    self.confirm_project_ssh_prompt();
                } else if self.inventory_edit_mode_open {
                    self.confirm_inventory_edit_mode_selection();
                } else if self.current_view() == View::Inventory
                    && matches!(
                        self.inventory_sub_tab,
                        InventorySubTab::Hosts | InventorySubTab::Groups
                    )
                {
                    self.handle_subtab_enter();
                } else if self.runtime_prompt_open {
                    self.select_runtime_candidate();
                } else if self.current_view() == View::Projects {
                    self.activate_selected_project();
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
            Action::ProjectSyncLog(line) => self.record_project_sync_log(line),
            Action::ProjectSyncFinished { success, message } => {
                self.project_sync_running = false;
                self.record_project_sync_log(message.clone());
                if let Some(pending) = self.pending_git_project.take() {
                    if success {
                        if !pending.root.is_dir() {
                            self.status_line = String::from(
                                "Git clone reported success, but destination directory is missing",
                            );
                        } else if self
                            .projects
                            .iter()
                            .any(|project| project.root == pending.root)
                        {
                            self.status_line = String::from(
                                "Git clone succeeded, but project root is already registered",
                            );
                        } else {
                            self.projects.push(ProjectDefinition {
                                name: pending.name.clone(),
                                root: pending.root.clone(),
                                inventory_sync_cmd: pending.inventory_sync_cmd,
                                vars_sync_cmd: pending.vars_sync_cmd,
                                ssh_private_key_file: None,
                                ssh_private_key_inline: None,
                            });
                            self.project_idx = self.projects.len().saturating_sub(1);
                            self.persist_projects();
                            self.status_line =
                                format!("Imported git project: {}", pending.name.clone());
                        }
                    } else {
                        self.status_line = format!("Git clone failed: {message}");
                    }
                } else {
                    self.status_line = message;
                    if success && self.project_idx == self.active_project_idx {
                        self.refresh_project();
                    }
                }
            }
            Action::RefreshProject => self.refresh_project(),
            Action::StartRun => self.start_run(tx),
            Action::StartTemplateRun => self.start_template_run(tx),
            Action::SaveTemplate => self.save_template_from_editor(),
            Action::DeleteTemplate => self.delete_selected_template(),
            Action::RunStarted {
                run_id,
                playbook,
                inventory,
                template_id,
                environment,
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
                        template_id,
                        environment,
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
                    let selected_run = self.run_idx == idx;
                    let run = &mut self.runs[idx];
                    let previous_len = run.logs.len();
                    let was_at_tail =
                        selected_run && self.log_cursor >= previous_len.saturating_sub(1);
                    run.logs.push(line);
                    if run.logs.len() > MAX_LOG_LINES {
                        let over = run.logs.len().saturating_sub(MAX_LOG_LINES);
                        run.logs.drain(0..over);
                        if selected_run {
                            self.log_cursor = self.log_cursor.saturating_sub(over);
                            if let Some(anchor) = self.log_anchor {
                                self.log_anchor = Some(anchor.saturating_sub(over));
                            }
                        }
                    }
                    if selected_run && (!self.log_select_mode || was_at_tail) {
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
        if self.template_editor_open {
            self.handle_template_editor_char(ch);
            return;
        }
        if self.project_ssh_open {
            self.push_project_ssh_char(ch);
            return;
        }
        if self.project_create_open {
            self.push_project_create_char(ch);
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
        if self.current_view() == View::Projects
            && self.pending_project_delete.is_some()
            && ch != 'D'
        {
            self.pending_project_delete = None;
        }
        if self.current_view() == View::Templates
            && self.pending_template_delete.is_some()
            && ch != 'D'
        {
            self.pending_template_delete = None;
        }

        if ch == 'q' {
            self.request_quit();
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
            // Handle groups sub-tab add-group input (reuses add_var prompt)
            if self.inventory_sub_tab == InventorySubTab::Groups && self.hosts_subtab_add_var_open {
                match ch {
                    '\n' => {
                        let name = self.hosts_subtab_add_var_buffer.trim().to_string();
                        if !name.is_empty()
                            && is_valid_inventory_key(&name)
                            && !is_reserved_inventory_group(&name)
                        {
                            if let Some(ref mut state) = self.inventory_edit_state {
                                if !state.groups.contains(&name) {
                                    state.groups.push(name.clone());
                                    state.assignments.entry(name.clone()).or_default();
                                    state.group_children.entry(name).or_default();
                                    state.dirty = true;
                                }
                            }
                        }
                        self.hosts_subtab_add_var_open = false;
                        self.hosts_subtab_add_var_buffer.clear();
                    }
                    _ if ch == '\x08' || ch == '\x7f' => {
                        self.hosts_subtab_add_var_buffer.pop();
                    }
                    _ if !ch.is_control() => {
                        self.hosts_subtab_add_var_buffer.push(ch);
                    }
                    _ => {}
                }
                return;
            }
            // Handle groups sub-tab add-host input
            if self.inventory_sub_tab == InventorySubTab::Groups && self.hosts_subtab_add_host_open
            {
                match ch {
                    '\n' => {
                        let name = self.hosts_subtab_add_host_buffer.trim().to_string();
                        if !name.is_empty() && is_valid_inventory_key(&name) {
                            if let Some(ref mut state) = self.inventory_edit_state {
                                if !state.hosts.contains(&name) {
                                    state.hosts.push(name.clone());
                                    state.host_vars.entry(name).or_default();
                                    state.dirty = true;
                                }
                            }
                        }
                        self.hosts_subtab_add_host_open = false;
                        self.hosts_subtab_add_host_buffer.clear();
                    }
                    _ if ch == '\x08' || ch == '\x7f' => {
                        self.hosts_subtab_add_host_buffer.pop();
                    }
                    _ if !ch.is_control() => {
                        self.hosts_subtab_add_host_buffer.push(ch);
                    }
                    _ => {}
                }
                return;
            }

            // Sub-tab switching
            match ch {
                '1' => {
                    self.inventory_sub_tab = InventorySubTab::Files;
                    return;
                }
                '2' => {
                    if self.selected_inventory_is_yaml() {
                        self.inventory_sub_tab = InventorySubTab::Hosts;
                        self.load_inventory_edit_state();
                    } else {
                        self.status_line =
                            String::from("Hosts sub-tab is only available for YAML inventories");
                    }
                    return;
                }
                '3' => {
                    if self.selected_inventory_is_yaml() {
                        self.inventory_sub_tab = InventorySubTab::Groups;
                        self.load_inventory_edit_state();
                    } else {
                        self.status_line =
                            String::from("Groups sub-tab is only available for YAML inventories");
                    }
                    return;
                }
                _ => {}
            }

            // Dispatch to sub-tab handlers
            match self.inventory_sub_tab {
                InventorySubTab::Hosts => {
                    self.handle_hosts_subtab_char(ch);
                    return;
                }
                InventorySubTab::Groups => {
                    self.handle_groups_subtab_char(ch);
                    return;
                }
                InventorySubTab::Files => {}
            }

            match ch {
                'n' => {
                    self.open_inventory_create_prompt();
                    return;
                }
                'e' => {
                    self.open_inventory_edit_mode_prompt();
                    return;
                }
                'D' => {
                    self.request_inventory_delete();
                    return;
                }
                'r' => {
                    self.status_line = String::from("Use Templates tab to run a selected template");
                    return;
                }
                _ => {}
            }
        }

        if self.current_view() == View::Projects {
            match ch {
                'n' => {
                    self.open_project_create_prompt(ProjectCreateMode::New);
                    return;
                }
                'f' => {
                    self.open_project_create_prompt(ProjectCreateMode::ExistingFs);
                    return;
                }
                'g' => {
                    self.open_project_create_prompt(ProjectCreateMode::Git);
                    return;
                }
                'a' => {
                    self.activate_selected_project();
                    return;
                }
                'e' => {
                    self.open_project_ssh_prompt();
                    return;
                }
                'D' => {
                    self.request_project_delete();
                    return;
                }
                'i' => {
                    self.start_project_sync(ProjectSyncKind::Inventory, tx);
                    return;
                }
                'v' => {
                    self.start_project_sync(ProjectSyncKind::Vars, tx);
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

        if self.current_view() == View::Templates {
            match ch {
                'n' => {
                    self.open_template_editor_new();
                    return;
                }
                'e' => {
                    self.open_template_editor_edit();
                    return;
                }
                't' => {
                    self.open_template_editor_edit();
                    return;
                }
                'D' => {
                    self.delete_selected_template();
                    return;
                }
                'r' => {
                    self.start_template_run(tx);
                    return;
                }
                'h' => {
                    self.templates_focus_runs = false;
                    return;
                }
                'l' => {
                    self.templates_focus_runs = true;
                    self.sync_run_selection_to_selected_template();
                    return;
                }
                'v' => {
                    self.toggle_log_select_mode();
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
                } else if self.current_view() == View::Templates {
                    self.templates_focus_runs = false;
                    self.sync_run_selection_to_selected_template();
                }
            }
            'l' => {
                self.view_idx = (self.view_idx + 1) % View::all().len();
                if self.current_view() == View::Playbooks {
                    self.playbooks_focus_runs = false;
                    self.sync_run_selection_to_selected_playbook();
                } else if self.current_view() == View::Templates {
                    self.templates_focus_runs = false;
                    self.sync_run_selection_to_selected_template();
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
            'J' => {
                if self.current_view() == View::Templates {
                    self.move_template_run_selection(1);
                } else {
                    self.select_playbook_run_offset(1);
                }
            }
            'K' => {
                if self.current_view() == View::Templates {
                    self.move_template_run_selection(-1);
                } else {
                    self.select_playbook_run_offset(-1);
                }
            }
            'r' => {
                if self.current_view() == View::Projects {
                    self.refresh_project();
                } else {
                    self.start_template_run(tx);
                }
            }
            'v' => {
                if self.current_view() == View::Playbooks || self.current_view() == View::Templates
                {
                    self.toggle_log_select_mode();
                }
            }
            'y' => self.copy_log_selection(),
            ' ' => self.mark_log_selection(),
            't' => {
                if self.current_view() == View::Templates {
                    self.open_template_editor_edit();
                } else if self.current_view() == View::Playbooks {
                    self.open_playbook_settings();
                }
            }
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
        if self.template_editor_open && self.template_editor_text_mode {
            self.template_editor_text_buffer.pop();
            return;
        }
        if self.project_ssh_open {
            self.backspace_project_ssh();
            return;
        }
        if self.project_create_open {
            self.backspace_project_create();
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
        if self.current_view() == View::Inventory
            && matches!(
                self.inventory_sub_tab,
                InventorySubTab::Hosts | InventorySubTab::Groups
            )
        {
            if self.hosts_subtab_editing {
                self.hosts_subtab_edit_buffer.pop();
            } else if self.hosts_subtab_add_host_open {
                self.hosts_subtab_add_host_buffer.pop();
            } else if self.hosts_subtab_add_var_open {
                self.hosts_subtab_add_var_buffer.pop();
            }
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
            if self.settings_field_accepts_multiline() {
                self.insert_settings_text_newline();
            } else {
                self.commit_settings_text_edit();
            }
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

    fn settings_field_accepts_multiline(&self) -> bool {
        self.settings_editor_field_idx == 11
    }

    fn begin_settings_text_edit(&mut self) {
        if !self.settings_editor_open || !self.settings_field_is_text() {
            return;
        }
        self.settings_editor_text_buffer = self.current_settings_text_value().unwrap_or_default();
        self.settings_editor_text_mode = true;
        self.status_line = if self.settings_field_accepts_multiline() {
            String::from(
                "Playbook settings: inline SSH key edit mode ON (type, Enter newline, Ctrl+S save)",
            )
        } else {
            String::from("Playbook settings: text edit mode ON (type, Backspace, Enter save)")
        };
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
        let value = if self.settings_field_accepts_multiline() {
            normalize_optional_multiline_text(self.settings_editor_text_buffer.clone())
        } else {
            normalize_optional_text(self.settings_editor_text_buffer.clone())
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

    fn insert_settings_text_newline(&mut self) {
        if !self.settings_editor_text_mode || !self.settings_field_accepts_multiline() {
            return;
        }
        self.settings_editor_text_buffer.push('\n');
    }

    fn current_settings_text_value(&self) -> Option<String> {
        let settings = self.selected_playbook_settings()?;
        match self.settings_editor_field_idx {
            6 => settings.limit,
            7 => settings.tags,
            8 => settings.extra_vars,
            9 => settings.extra_args,
            10 => settings.ssh_private_key_file,
            11 => settings.ssh_private_key_inline,
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
            10 => settings.ssh_private_key_file = value,
            11 => settings.ssh_private_key_inline = value,
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
            View::Projects => {
                self.project_idx = self.project_idx.saturating_sub(1);
                self.pending_project_delete = None;
            }
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
            View::Templates => {
                if self.templates_focus_runs {
                    self.move_template_run_selection(-1);
                } else {
                    let filtered = self.filtered_template_indices();
                    if let Some(pos) = filtered.iter().position(|i| *i == self.template_idx) {
                        if pos > 0 {
                            self.template_idx = filtered[pos - 1];
                        }
                    }
                    self.pending_template_delete = None;
                    self.sync_run_selection_to_selected_template();
                }
            }
            _ => {}
        }
    }

    fn move_selection_down(&mut self) {
        match self.current_view() {
            View::Projects => {
                if !self.projects.is_empty() {
                    self.project_idx = min(self.project_idx + 1, self.projects.len() - 1);
                }
                self.pending_project_delete = None;
            }
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
            View::Templates => {
                if self.templates_focus_runs {
                    self.move_template_run_selection(1);
                } else {
                    let filtered = self.filtered_template_indices();
                    if let Some(pos) = filtered.iter().position(|i| *i == self.template_idx) {
                        if pos + 1 < filtered.len() {
                            self.template_idx = filtered[pos + 1];
                        }
                    }
                    self.pending_template_delete = None;
                    self.sync_run_selection_to_selected_template();
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

    fn scroll_logs_by(&mut self, delta: isize) {
        if matches!(self.current_view(), View::Dashboard | View::Projects) {
            return;
        }
        let Some(run_len) = self.runs.get(self.run_idx).map(|run| run.logs.len()) else {
            self.status_line = String::from("No run selected");
            return;
        };
        if run_len == 0 {
            self.status_line = String::from("No logs yet for this run.");
            return;
        }

        if !self.log_select_mode {
            self.log_select_mode = true;
            self.log_anchor = None;
            self.sync_log_cursor_to_selected_run();
        }

        let max_idx = run_len.saturating_sub(1);
        let step = delta.unsigned_abs();
        if delta.is_negative() {
            self.log_cursor = self.log_cursor.saturating_sub(step);
        } else {
            self.log_cursor = min(self.log_cursor.saturating_add(step), max_idx);
        }
    }

    fn follow_logs_latest(&mut self) {
        if matches!(self.current_view(), View::Dashboard | View::Projects) {
            return;
        }
        self.log_select_mode = false;
        self.log_anchor = None;
        self.sync_log_cursor_to_selected_run();
        self.status_line = String::from("Live log follow mode ON");
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
        if !matches!(self.current_view(), View::Playbooks | View::Templates)
            || self.runtime_prompt_open
            || self.settings_editor_open
            || self.runs.is_empty()
        {
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
        if self.current_view() != View::Playbooks || !self.log_select_mode || self.runs.is_empty() {
            return;
        }
        let Some(idx) = self.log_index_from_view_row(row, viewport_height) else {
            return;
        };
        self.log_cursor = idx;
    }

    fn log_mouse_up(&mut self) {
        if self.current_view() != View::Playbooks
            || !self.log_select_mode
            || self.log_anchor.is_none()
        {
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

    fn project_create_last_field_idx(&self) -> usize {
        match self.project_create_mode {
            ProjectCreateMode::Git => 4,
            ProjectCreateMode::New | ProjectCreateMode::ExistingFs => 3,
        }
    }

    fn open_project_create_prompt(&mut self, mode: ProjectCreateMode) {
        if self.current_view() != View::Projects {
            self.status_line = String::from("Project create is available in Projects tab");
            return;
        }
        if self.project_sync_running {
            self.status_line = String::from("Wait for current project sync/clone to finish");
            return;
        }
        self.pending_project_delete = None;
        self.project_create_open = true;
        self.project_create_mode = mode;
        self.project_create_field_idx = 0;
        self.project_create_buffer_name.clear();
        self.project_create_buffer_git_url.clear();
        self.project_create_buffer_root = match mode {
            ProjectCreateMode::New => {
                let suggested = self.cwd.join("projects").join("project-name");
                display_path(&self.cwd, &suggested)
            }
            ProjectCreateMode::ExistingFs | ProjectCreateMode::Git => String::new(),
        };
        self.project_create_buffer_inventory_sync.clear();
        self.project_create_buffer_vars_sync.clear();
        self.status_line = format!(
            "{}: fill all fields and press Enter to continue",
            mode.title()
        );
    }

    fn cancel_project_create_prompt(&mut self) {
        self.project_create_open = false;
        self.project_create_field_idx = 0;
        self.project_create_mode = ProjectCreateMode::New;
        self.project_create_buffer_name.clear();
        self.project_create_buffer_git_url.clear();
        self.project_create_buffer_root.clear();
        self.project_create_buffer_inventory_sync.clear();
        self.project_create_buffer_vars_sync.clear();
        self.status_line = String::from("Project create cancelled");
    }

    fn open_project_ssh_prompt(&mut self) {
        if self.current_view() != View::Projects {
            self.status_line =
                String::from("Project SSH key settings are available in Projects tab");
            return;
        }
        let Some(project) = self.projects.get(self.project_idx).cloned() else {
            self.status_line = String::from("No project selected");
            return;
        };
        self.pending_project_delete = None;
        self.project_ssh_open = true;
        self.project_ssh_field_idx = 0;
        self.project_ssh_buffer_file = project.ssh_private_key_file.unwrap_or_default();
        self.project_ssh_buffer_inline = project.ssh_private_key_inline.unwrap_or_default();
        self.project_ssh_target_root = Some(project.root);
        self.status_line = format!(
            "Project SSH key settings: {} (Ctrl+S save, Esc cancel)",
            project.name
        );
    }

    fn cancel_project_ssh_prompt(&mut self) {
        self.project_ssh_open = false;
        self.project_ssh_field_idx = 0;
        self.project_ssh_buffer_file.clear();
        self.project_ssh_buffer_inline.clear();
        self.project_ssh_target_root = None;
        self.status_line = String::from("Project SSH key settings cancelled");
    }

    fn save_project_ssh_prompt(&mut self) {
        if !self.project_ssh_open {
            return;
        }
        let Some(target_root) = self.project_ssh_target_root.clone() else {
            self.cancel_project_ssh_prompt();
            return;
        };
        let Some(idx) = self.projects.iter().position(|p| p.root == target_root) else {
            self.cancel_project_ssh_prompt();
            self.status_line = String::from("Project no longer exists");
            return;
        };

        let file = normalize_optional_text(self.project_ssh_buffer_file.clone());
        let inline = normalize_optional_multiline_text(self.project_ssh_buffer_inline.clone());
        self.projects[idx].ssh_private_key_file = file;
        self.projects[idx].ssh_private_key_inline = inline;
        let name = self.projects[idx].name.clone();
        self.persist_projects();

        self.project_ssh_open = false;
        self.project_ssh_field_idx = 0;
        self.project_ssh_buffer_file.clear();
        self.project_ssh_buffer_inline.clear();
        self.project_ssh_target_root = None;
        self.status_line = format!("Project SSH key settings updated: {name}");
    }

    fn confirm_project_ssh_prompt(&mut self) {
        if !self.project_ssh_open {
            return;
        }
        if self.project_ssh_field_idx == 0 {
            self.project_ssh_field_idx = 1;
            return;
        }
        self.insert_project_ssh_newline();
    }

    fn active_project_ssh_buffer_mut(&mut self) -> &mut String {
        if self.project_ssh_field_idx == 0 {
            &mut self.project_ssh_buffer_file
        } else {
            &mut self.project_ssh_buffer_inline
        }
    }

    fn push_project_ssh_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        self.active_project_ssh_buffer_mut().push(ch);
    }

    fn backspace_project_ssh(&mut self) {
        self.active_project_ssh_buffer_mut().pop();
    }

    fn insert_project_ssh_newline(&mut self) {
        if self.project_ssh_field_idx != 1 {
            return;
        }
        self.project_ssh_buffer_inline.push('\n');
    }

    fn active_project_create_buffer_mut(&mut self) -> &mut String {
        match self.project_create_mode {
            ProjectCreateMode::Git => match self.project_create_field_idx {
                0 => &mut self.project_create_buffer_name,
                1 => &mut self.project_create_buffer_git_url,
                2 => &mut self.project_create_buffer_root,
                3 => &mut self.project_create_buffer_inventory_sync,
                _ => &mut self.project_create_buffer_vars_sync,
            },
            ProjectCreateMode::New | ProjectCreateMode::ExistingFs => {
                match self.project_create_field_idx {
                    0 => &mut self.project_create_buffer_name,
                    1 => &mut self.project_create_buffer_root,
                    2 => &mut self.project_create_buffer_inventory_sync,
                    _ => &mut self.project_create_buffer_vars_sync,
                }
            }
        }
    }

    fn push_project_create_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        self.active_project_create_buffer_mut().push(ch);
    }

    fn backspace_project_create(&mut self) {
        self.active_project_create_buffer_mut().pop();
    }

    fn confirm_project_create(&mut self, tx: &UnboundedSender<Action>) {
        if !self.project_create_open {
            return;
        }
        if self.project_create_field_idx < self.project_create_last_field_idx() {
            self.project_create_field_idx += 1;
            return;
        }

        match self.project_create_mode {
            ProjectCreateMode::New => self.submit_new_project(),
            ProjectCreateMode::ExistingFs => self.submit_existing_project(),
            ProjectCreateMode::Git => self.submit_git_project(tx),
        }
    }

    fn parsed_project_root(&self) -> Result<PathBuf, String> {
        let root_input = self.project_create_buffer_root.trim();
        if root_input.is_empty() {
            return Err(String::from("Project root path is required"));
        }
        let root = if Path::new(root_input).is_absolute() {
            PathBuf::from(root_input)
        } else {
            self.cwd.join(root_input)
        };
        Ok(root)
    }

    fn parsed_project_name(&self, root: &Path) -> String {
        if self.project_create_buffer_name.trim().is_empty() {
            root.file_name()
                .and_then(|v| v.to_str())
                .filter(|v| !v.trim().is_empty())
                .unwrap_or("Project")
                .to_string()
        } else {
            self.project_create_buffer_name.trim().to_string()
        }
    }

    fn parsed_project_sync_commands(&self) -> (Option<String>, Option<String>) {
        let inventory_sync_cmd =
            normalize_optional_text(self.project_create_buffer_inventory_sync.trim().to_string());
        let vars_sync_cmd =
            normalize_optional_text(self.project_create_buffer_vars_sync.trim().to_string());
        (inventory_sync_cmd, vars_sync_cmd)
    }

    fn submit_new_project(&mut self) {
        let root = match self.parsed_project_root() {
            Ok(root) => root,
            Err(err) => {
                self.status_line = err;
                self.project_create_field_idx = 1;
                return;
            }
        };
        if self.projects.iter().any(|project| project.root == root) {
            self.status_line = String::from("A project with this root already exists");
            self.project_create_field_idx = 1;
            return;
        }
        if root.exists() {
            if !root.is_dir() {
                self.status_line = String::from("Project root must be a directory");
                self.project_create_field_idx = 1;
                return;
            }
        } else if let Err(err) = fs::create_dir_all(&root) {
            self.status_line = format!("Failed to create project root: {err}");
            self.project_create_field_idx = 1;
            return;
        }
        if let Err(err) = ensure_ansible_project_layout(&root) {
            self.status_line = format!("Failed to create Ansible project structure: {err}");
            self.project_create_field_idx = 1;
            return;
        }

        let name = self.parsed_project_name(&root);
        let (inventory_sync_cmd, vars_sync_cmd) = self.parsed_project_sync_commands();
        self.projects.push(ProjectDefinition {
            name: name.clone(),
            root,
            inventory_sync_cmd,
            vars_sync_cmd,
            ssh_private_key_file: None,
            ssh_private_key_inline: None,
        });
        self.project_idx = self.projects.len().saturating_sub(1);
        self.cancel_project_create_prompt();
        self.persist_projects();
        self.status_line = format!("Created project with standard layout: {name}");
    }

    fn submit_existing_project(&mut self) {
        let root = match self.parsed_project_root() {
            Ok(root) => root,
            Err(err) => {
                self.status_line = err;
                self.project_create_field_idx = 1;
                return;
            }
        };
        if self.projects.iter().any(|project| project.root == root) {
            self.status_line = String::from("A project with this root already exists");
            self.project_create_field_idx = 1;
            return;
        }
        if !root.exists() {
            self.status_line = String::from("Project root does not exist");
            self.project_create_field_idx = 1;
            return;
        }
        if !root.is_dir() {
            self.status_line = String::from("Project root must be a directory");
            self.project_create_field_idx = 1;
            return;
        }

        let name = self.parsed_project_name(&root);
        let (inventory_sync_cmd, vars_sync_cmd) = self.parsed_project_sync_commands();
        self.projects.push(ProjectDefinition {
            name: name.clone(),
            root,
            inventory_sync_cmd,
            vars_sync_cmd,
            ssh_private_key_file: None,
            ssh_private_key_inline: None,
        });
        self.project_idx = self.projects.len().saturating_sub(1);
        self.cancel_project_create_prompt();
        self.persist_projects();
        self.status_line = format!("Imported existing project: {name}");
    }

    fn submit_git_project(&mut self, tx: &UnboundedSender<Action>) {
        let git_url = self.project_create_buffer_git_url.trim().to_string();
        if git_url.is_empty() {
            self.status_line = String::from("Git URL is required");
            self.project_create_field_idx = 1;
            return;
        }
        let root = match self.parsed_project_root() {
            Ok(root) => root,
            Err(err) => {
                self.status_line = err;
                self.project_create_field_idx = 2;
                return;
            }
        };
        if self.projects.iter().any(|project| project.root == root) {
            self.status_line = String::from("A project with this root already exists");
            self.project_create_field_idx = 2;
            return;
        }
        if root.exists() {
            self.status_line =
                String::from("Destination already exists; choose an empty/non-existent path");
            self.project_create_field_idx = 2;
            return;
        }
        if let Some(parent) = root.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                self.status_line = format!("Failed to create parent directory: {err}");
                self.project_create_field_idx = 2;
                return;
            }
        }

        let name = self.parsed_project_name(&root);
        let (inventory_sync_cmd, vars_sync_cmd) = self.parsed_project_sync_commands();
        self.pending_git_project = Some(PendingGitProject {
            name: name.clone(),
            root: root.clone(),
            inventory_sync_cmd,
            vars_sync_cmd,
        });
        self.project_sync_running = true;
        self.record_project_sync_log(format!("Cloning {git_url} -> {}", root.display()));
        self.cancel_project_create_prompt();
        spawn_git_clone(git_url, root, tx.clone());
        self.status_line = format!("Started cloning git project: {name}");
    }

    fn activate_selected_project(&mut self) {
        if self.current_view() != View::Projects {
            return;
        }
        if self.project_sync_running {
            self.status_line = String::from("Wait for current project sync/clone to finish");
            return;
        }
        if self.project_idx >= self.projects.len() {
            self.status_line = String::from("No project selected");
            return;
        }
        self.pending_project_delete = None;
        if self.project_idx == self.active_project_idx {
            self.status_line = format!("Project already active: {}", self.active_project_name());
            return;
        }
        self.activate_project_idx(self.project_idx);
    }

    fn request_project_delete(&mut self) {
        if self.current_view() != View::Projects {
            self.status_line = String::from("Project delete is available in Projects tab");
            return;
        }
        if self.project_sync_running {
            self.status_line = String::from("Wait for current project sync/clone to finish");
            return;
        }
        if self.projects.len() <= 1 {
            self.status_line = String::from("Refusing to delete the last project");
            return;
        }
        let Some(project) = self.projects.get(self.project_idx).cloned() else {
            self.status_line = String::from("No project selected");
            return;
        };

        if self
            .pending_project_delete
            .as_ref()
            .map(|root| root == &project.root)
            .unwrap_or(false)
        {
            self.pending_project_delete = None;
            self.delete_project(project.root);
            return;
        }

        self.pending_project_delete = Some(project.root.clone());
        self.status_line = format!("Press Shift+D again to delete project {}", project.name);
    }

    fn delete_project(&mut self, project_root: PathBuf) {
        let Some(idx) = self
            .projects
            .iter()
            .position(|project| project.root == project_root)
        else {
            self.status_line = String::from("Selected project is no longer available");
            return;
        };
        if self.projects.len() <= 1 {
            self.status_line = String::from("Refusing to delete the last project");
            return;
        }

        let removed = self.projects.remove(idx);
        let removed_was_active = idx == self.active_project_idx;
        self.pending_project_delete = None;

        if removed_was_active {
            self.active_project_idx = min(idx, self.projects.len().saturating_sub(1));
            self.project_idx = self.active_project_idx;
            self.load_active_project_state();
            self.persist_projects();
            self.status_line = format!(
                "Deleted project {}. Active project: {}",
                removed.name,
                self.active_project_name()
            );
            return;
        }

        if idx < self.active_project_idx {
            self.active_project_idx = self.active_project_idx.saturating_sub(1);
        }
        if self.project_idx >= self.projects.len() {
            self.project_idx = self.projects.len().saturating_sub(1);
        }
        self.persist_projects();
        self.status_line = format!("Deleted project {}", removed.name);
    }

    fn start_project_sync(&mut self, kind: ProjectSyncKind, tx: &UnboundedSender<Action>) {
        if self.current_view() != View::Projects {
            self.status_line = String::from("Project sync is available in Projects tab");
            return;
        }
        if self.project_sync_running {
            self.status_line = String::from("A project sync is already running");
            return;
        }
        let Some(project) = self.projects.get(self.project_idx).cloned() else {
            self.status_line = String::from("No project selected");
            return;
        };
        let command_line = match kind {
            ProjectSyncKind::Inventory => project.inventory_sync_cmd.clone(),
            ProjectSyncKind::Vars => project.vars_sync_cmd.clone(),
        };
        let Some(command_line) = command_line else {
            self.status_line = format!("No {} sync command configured", kind.title());
            return;
        };

        self.project_sync_running = true;
        self.record_project_sync_log(format!(
            "Starting {} sync for project {}",
            kind.title(),
            project.name
        ));
        spawn_project_sync(
            project.root,
            command_line,
            kind.title().to_string(),
            tx.clone(),
        );
    }

    fn record_project_sync_log(&mut self, line: String) {
        self.project_sync_logs.push(line);
        if self.project_sync_logs.len() > MAX_PROJECT_SYNC_LOG_LINES {
            let over = self
                .project_sync_logs
                .len()
                .saturating_sub(MAX_PROJECT_SYNC_LOG_LINES);
            self.project_sync_logs.drain(0..over);
        }
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
                String::from("No inventories found. Add inventory files under ./inventory.");
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

        let project_root = self.active_project_root().to_path_buf();
        let playbook = display_path(&project_root, playbook_path);
        let inventory = display_path(&project_root, &inventory_path);
        let settings = self
            .playbook_settings
            .get(&playbook)
            .cloned()
            .unwrap_or_else(|| self.default_settings());
        let (ssh_private_key_file, ssh_private_key_inline) =
            self.effective_ssh_private_key_settings(&settings);

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
        options.ssh_private_key_file = ssh_private_key_file;
        options.ssh_private_key_inline = ssh_private_key_inline;

        self.status_line = format!("Starting run #{run_id}...");
        spawn_ansible_run(
            RunRequest {
                run_id,
                cwd: project_root,
                playbook,
                inventory,
                options,
                template_id: None,
                environment: Some(self.active_project_name()),
            },
            tx.clone(),
        );
    }

    fn effective_ssh_private_key_settings(
        &self,
        settings: &PlaybookSettings,
    ) -> (Option<String>, Option<String>) {
        if settings.ssh_private_key_file.is_some() || settings.ssh_private_key_inline.is_some() {
            return (
                settings.ssh_private_key_file.clone(),
                settings.ssh_private_key_inline.clone(),
            );
        }
        self.projects
            .get(self.active_project_idx)
            .map(|project| {
                (
                    project.ssh_private_key_file.clone(),
                    project.ssh_private_key_inline.clone(),
                )
            })
            .unwrap_or((None, None))
    }

    fn refresh_project(&mut self) {
        let project_root = self.active_project_root().to_path_buf();
        let (inventories, playbooks, vars_files) = discover_project(&project_root);
        self.apply_discovered_project(inventories, playbooks, vars_files);
        self.status_line = format!(
            "Loaded {} playbooks, {} inventories, {} vars files ({})",
            self.playbooks.len(),
            self.inventories.len(),
            self.vars_files.len(),
            self.active_project_name()
        );
    }

    fn auto_refresh_project(&mut self) {
        if self.last_auto_discovery_at.elapsed() < AUTO_DISCOVERY_INTERVAL {
            return;
        }
        self.last_auto_discovery_at = Instant::now();
        let project_root = self.active_project_root().to_path_buf();
        let (inventories, playbooks, vars_files) = discover_project(&project_root);
        if inventories == self.inventories
            && playbooks == self.playbooks
            && vars_files == self.vars_files
        {
            return;
        }
        self.apply_discovered_project(inventories, playbooks, vars_files);
        self.status_line = format!(
            "Project updated: {} playbooks, {} inventories, {} vars files ({})",
            self.playbooks.len(),
            self.inventories.len(),
            self.vars_files.len(),
            self.active_project_name()
        );
    }

    fn apply_discovered_project(
        &mut self,
        inventories: Vec<PathBuf>,
        playbooks: Vec<PathBuf>,
        vars_files: Vec<PathBuf>,
    ) {
        let project_root = self.active_project_root().to_path_buf();
        self.inventories = inventories;
        self.playbooks = playbooks;
        self.vars_files = vars_files;
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
            .map(|p| display_path(&project_root, p))
            .collect::<HashSet<_>>();
        self.selected_run_by_playbook
            .retain(|playbook, _| known_playbooks.contains(playbook));
        self.selected_inventory_by_playbook
            .retain(|playbook, inventory| {
                known_playbooks.contains(playbook)
                    && self
                        .inventories
                        .iter()
                        .any(|path| display_path(&project_root, path) == *inventory)
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
                    display_path(self.active_project_root(), &path)
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
                self.status_line = format!(
                    "Saved inventory {}",
                    display_path(self.active_project_root(), &path)
                );
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
        const MODE_COUNT: usize = 2;
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
            'e' => {
                self.inventory_edit_mode_idx = 0;
                self.confirm_inventory_edit_mode_selection();
            }
            't' => {
                self.inventory_edit_mode_idx = 1;
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
                    display_path(self.active_project_root(), &path)
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

    pub fn selected_inventory_is_yaml(&self) -> bool {
        self.inventories
            .get(self.inventory_idx)
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_ascii_lowercase().as_str(), "yml" | "yaml"))
            .unwrap_or(false)
    }
    fn load_inventory_edit_state(&mut self) {
        let Some(path) = self.inventories.get(self.inventory_idx).cloned() else {
            self.inventory_edit_state = None;
            return;
        };
        if let Some(ref state) = self.inventory_edit_state {
            if state.path == path {
                return;
            }
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(err) => {
                self.status_line = format!("Failed to read inventory: {err}");
                self.inventory_edit_state = None;
                return;
            }
        };
        let parsed = match parse_inventory_yaml_for_builder(&content) {
            Ok(p) => p,
            Err(err) => {
                self.status_line = format!("Parse failed ({err}). Use text editor for this file.");
                self.inventory_edit_state = None;
                return;
            }
        };
        self.inventory_edit_state = Some(InventoryEditState {
            path,
            hosts: parsed.hosts,
            host_vars: parsed.host_vars,
            groups: parsed.groups,
            assignments: parsed.assignments,
            group_children: parsed.group_children,
            dirty: false,
        });
        self.hosts_subtab_idx = 0;
        self.hosts_subtab_focus_detail = false;
        self.hosts_subtab_field_idx = 0;
        self.hosts_subtab_editing = false;
        self.hosts_subtab_edit_buffer.clear();
        self.hosts_subtab_add_host_open = false;
        self.hosts_subtab_add_host_buffer.clear();
        self.hosts_subtab_add_var_open = false;
        self.hosts_subtab_add_var_buffer.clear();
        self.groups_subtab_focus = GroupsFocus::Tree;
        self.groups_subtab_tree_idx = 0;
        self.groups_subtab_group_idx = 0;
        self.groups_subtab_host_idx = 0;
        self.groups_subtab_target_group = None;
    }

    fn handle_subtab_enter(&mut self) {
        // Handle Enter in input modes
        if self.hosts_subtab_add_host_open {
            self.handle_hosts_subtab_char('\n');
            return;
        }
        if self.hosts_subtab_add_var_open {
            if self.inventory_sub_tab == InventorySubTab::Groups {
                // Add group in groups sub-tab
                let name = self.hosts_subtab_add_var_buffer.trim().to_string();
                if !name.is_empty()
                    && is_valid_inventory_key(&name)
                    && !is_reserved_inventory_group(&name)
                {
                    if let Some(ref mut state) = self.inventory_edit_state {
                        if !state.groups.contains(&name) {
                            state.groups.push(name.clone());
                            state.assignments.entry(name.clone()).or_default();
                            state.group_children.entry(name).or_default();
                            state.dirty = true;
                        }
                    }
                }
                self.hosts_subtab_add_var_open = false;
                self.hosts_subtab_add_var_buffer.clear();
            } else {
                self.handle_hosts_subtab_char('\n');
            }
            return;
        }
        if self.hosts_subtab_editing {
            self.handle_hosts_subtab_char('\n');
            return;
        }

        // Non-editing Enter: begin editing in hosts, or toggle in groups
        match self.inventory_sub_tab {
            InventorySubTab::Hosts => {
                if self.hosts_subtab_focus_detail {
                    self.handle_hosts_subtab_char('e');
                }
            }
            InventorySubTab::Groups => {
                self.handle_groups_subtab_char(' ');
            }
            InventorySubTab::Files => {}
        }
    }

    fn handle_hosts_subtab_char(&mut self, ch: char) {
        if self.hosts_subtab_add_host_open {
            match ch {
                '\n' => {
                    let name = self.hosts_subtab_add_host_buffer.trim().to_string();
                    if !name.is_empty() && is_valid_inventory_key(&name) {
                        if let Some(ref mut state) = self.inventory_edit_state {
                            if !state.hosts.contains(&name) {
                                state.hosts.push(name.clone());
                                state.host_vars.entry(name).or_default();
                                state.dirty = true;
                                self.hosts_subtab_idx = state.hosts.len() - 1;
                            }
                        }
                    }
                    self.hosts_subtab_add_host_open = false;
                    self.hosts_subtab_add_host_buffer.clear();
                }
                _ if ch == '\x08' || ch == '\x7f' => {
                    self.hosts_subtab_add_host_buffer.pop();
                }
                _ if !ch.is_control() => {
                    self.hosts_subtab_add_host_buffer.push(ch);
                }
                _ => {}
            }
            return;
        }

        if self.hosts_subtab_add_var_open {
            match ch {
                '\n' => {
                    let key = self.hosts_subtab_add_var_buffer.trim().to_string();
                    if !key.is_empty() {
                        if let Some(ref mut state) = self.inventory_edit_state {
                            if let Some(host) = state.hosts.get(self.hosts_subtab_idx).cloned() {
                                let vars = state.host_vars.entry(host).or_default();
                                if !vars.custom_vars.iter().any(|(k, _)| k == &key) {
                                    vars.custom_vars.push((key, String::new()));
                                    state.dirty = true;
                                    self.hosts_subtab_field_idx = 3 + vars.custom_vars.len();
                                }
                            }
                        }
                    }
                    self.hosts_subtab_add_var_open = false;
                    self.hosts_subtab_add_var_buffer.clear();
                }
                _ if ch == '\x08' || ch == '\x7f' => {
                    self.hosts_subtab_add_var_buffer.pop();
                }
                _ if !ch.is_control() => {
                    self.hosts_subtab_add_var_buffer.push(ch);
                }
                _ => {}
            }
            return;
        }

        if self.hosts_subtab_editing {
            match ch {
                '\n' => {
                    if let Some(ref mut state) = self.inventory_edit_state {
                        if let Some(host) = state.hosts.get(self.hosts_subtab_idx).cloned() {
                            let vars = state.host_vars.entry(host).or_default();
                            let buf = self.hosts_subtab_edit_buffer.clone();
                            match self.hosts_subtab_field_idx {
                                0 => vars.ansible_host = buf,
                                1 => vars.ansible_user = buf,
                                2 => vars.ansible_port = buf.parse::<u16>().ok(),
                                3 => vars.ansible_connection = buf,
                                n => {
                                    let ci = n - 4;
                                    if let Some(entry) = vars.custom_vars.get_mut(ci) {
                                        entry.1 = buf;
                                    }
                                }
                            }
                            state.dirty = true;
                        }
                    }
                    self.hosts_subtab_editing = false;
                    self.hosts_subtab_edit_buffer.clear();
                }
                _ if ch == '\x08' || ch == '\x7f' => {
                    self.hosts_subtab_edit_buffer.pop();
                }
                _ if !ch.is_control() => {
                    self.hosts_subtab_edit_buffer.push(ch);
                }
                _ => {}
            }
            return;
        }

        match ch {
            'h' => {
                self.hosts_subtab_focus_detail = false;
            }
            'l' => {
                self.hosts_subtab_focus_detail = true;
            }
            'j' => {
                if self.hosts_subtab_focus_detail {
                    let max_field = self.hosts_subtab_max_field_idx();
                    if max_field > 0 {
                        self.hosts_subtab_field_idx =
                            min(self.hosts_subtab_field_idx + 1, max_field);
                    }
                } else if let Some(ref state) = self.inventory_edit_state {
                    if !state.hosts.is_empty() {
                        self.hosts_subtab_idx =
                            min(self.hosts_subtab_idx + 1, state.hosts.len() - 1);
                        self.hosts_subtab_field_idx = 0;
                    }
                }
            }
            'k' => {
                if self.hosts_subtab_focus_detail {
                    self.hosts_subtab_field_idx = self.hosts_subtab_field_idx.saturating_sub(1);
                } else {
                    self.hosts_subtab_idx = self.hosts_subtab_idx.saturating_sub(1);
                    self.hosts_subtab_field_idx = 0;
                }
            }
            'e' | '\n' => {
                if self.hosts_subtab_focus_detail {
                    if let Some(ref state) = self.inventory_edit_state {
                        if let Some(host) = state.hosts.get(self.hosts_subtab_idx) {
                            let vars = state.host_vars.get(host);
                            self.hosts_subtab_edit_buffer = match self.hosts_subtab_field_idx {
                                0 => vars.map(|v| v.ansible_host.clone()).unwrap_or_default(),
                                1 => vars.map(|v| v.ansible_user.clone()).unwrap_or_default(),
                                2 => vars
                                    .and_then(|v| v.ansible_port)
                                    .map(|p| p.to_string())
                                    .unwrap_or_default(),
                                3 => vars
                                    .map(|v| v.ansible_connection.clone())
                                    .unwrap_or_default(),
                                n => vars
                                    .and_then(|v| v.custom_vars.get(n - 4))
                                    .map(|(_, val)| val.clone())
                                    .unwrap_or_default(),
                            };
                            self.hosts_subtab_editing = true;
                        }
                    }
                }
            }
            'n' => {
                if !self.hosts_subtab_focus_detail {
                    self.hosts_subtab_add_host_open = true;
                    self.hosts_subtab_add_host_buffer.clear();
                }
            }
            'a' => {
                if self.hosts_subtab_focus_detail {
                    self.hosts_subtab_add_var_open = true;
                    self.hosts_subtab_add_var_buffer.clear();
                }
            }
            'D' => {
                if let Some(ref mut state) = self.inventory_edit_state {
                    if self.hosts_subtab_focus_detail {
                        if self.hosts_subtab_field_idx >= 4 {
                            let ci = self.hosts_subtab_field_idx - 4;
                            if let Some(host) = state.hosts.get(self.hosts_subtab_idx).cloned() {
                                if let Some(vars) = state.host_vars.get_mut(&host) {
                                    if ci < vars.custom_vars.len() {
                                        vars.custom_vars.remove(ci);
                                        state.dirty = true;
                                        let max = self.hosts_subtab_max_field_idx();
                                        if self.hosts_subtab_field_idx > max {
                                            self.hosts_subtab_field_idx = max;
                                        }
                                    }
                                }
                            }
                        }
                    } else if !state.hosts.is_empty() {
                        let removed = state.hosts.remove(self.hosts_subtab_idx);
                        state.host_vars.remove(&removed);
                        for assigned in state.assignments.values_mut() {
                            assigned.retain(|h| h != &removed);
                        }
                        state.dirty = true;
                        if self.hosts_subtab_idx >= state.hosts.len() && !state.hosts.is_empty() {
                            self.hosts_subtab_idx = state.hosts.len() - 1;
                        }
                        self.hosts_subtab_field_idx = 0;
                    }
                }
            }
            _ => {}
        }
    }

    fn hosts_subtab_max_field_idx(&self) -> usize {
        if let Some(ref state) = self.inventory_edit_state {
            if let Some(host) = state.hosts.get(self.hosts_subtab_idx) {
                let custom_len = state
                    .host_vars
                    .get(host)
                    .map(|v| v.custom_vars.len())
                    .unwrap_or(0);
                return 3 + custom_len;
            }
        }
        3
    }

    fn handle_groups_subtab_char(&mut self, ch: char) {
        match ch {
            'h' => {
                self.groups_subtab_focus = match self.groups_subtab_focus {
                    GroupsFocus::Tree => GroupsFocus::Hosts,
                    GroupsFocus::Groups => GroupsFocus::Tree,
                    GroupsFocus::Hosts => GroupsFocus::Groups,
                };
            }
            'l' => {
                self.groups_subtab_focus = match self.groups_subtab_focus {
                    GroupsFocus::Tree => GroupsFocus::Groups,
                    GroupsFocus::Groups => GroupsFocus::Hosts,
                    GroupsFocus::Hosts => GroupsFocus::Tree,
                };
            }
            'j' => match self.groups_subtab_focus {
                GroupsFocus::Tree => {
                    let len = self.groups_subtab_tree_nodes().len();
                    if len > 0 {
                        self.groups_subtab_tree_idx = min(self.groups_subtab_tree_idx + 1, len - 1);
                        self.sync_groups_subtab_tree_selection();
                    }
                }
                GroupsFocus::Groups => {
                    let len = self.groups_subtab_candidate_groups().len();
                    if len > 0 {
                        self.groups_subtab_group_idx =
                            min(self.groups_subtab_group_idx + 1, len - 1);
                    }
                }
                GroupsFocus::Hosts => {
                    let len = self
                        .inventory_edit_state
                        .as_ref()
                        .map(|s| s.hosts.len())
                        .unwrap_or(0);
                    if len > 0 {
                        self.groups_subtab_host_idx = min(self.groups_subtab_host_idx + 1, len - 1);
                    }
                }
            },
            'k' => match self.groups_subtab_focus {
                GroupsFocus::Tree => {
                    self.groups_subtab_tree_idx = self.groups_subtab_tree_idx.saturating_sub(1);
                    self.sync_groups_subtab_tree_selection();
                }
                GroupsFocus::Groups => {
                    self.groups_subtab_group_idx = self.groups_subtab_group_idx.saturating_sub(1);
                }
                GroupsFocus::Hosts => {
                    self.groups_subtab_host_idx = self.groups_subtab_host_idx.saturating_sub(1);
                }
            },
            ' ' => {
                self.toggle_groups_subtab_attachment();
            }
            'n' => {
                match self.groups_subtab_focus {
                    GroupsFocus::Groups | GroupsFocus::Tree => {
                        // Add group via prompt reuse - open the add_var prompt repurposed
                        self.hosts_subtab_add_var_open = true;
                        self.hosts_subtab_add_var_buffer.clear();
                        self.status_line = String::from("Type new group name and press Enter");
                    }
                    GroupsFocus::Hosts => {
                        self.hosts_subtab_add_host_open = true;
                        self.hosts_subtab_add_host_buffer.clear();
                        self.status_line = String::from("Type new host name and press Enter");
                    }
                }
            }
            'D' => {
                self.delete_groups_subtab_item();
            }
            'd' => {
                self.detach_groups_subtab_item();
            }
            _ => {}
        }
    }

    pub fn groups_subtab_tree_nodes(&self) -> Vec<(Option<String>, usize)> {
        let Some(ref state) = self.inventory_edit_state else {
            return vec![(None, 0)];
        };
        compute_tree_nodes(&state.groups, &state.group_children)
    }

    pub fn groups_subtab_candidate_groups(&self) -> Vec<String> {
        let Some(ref state) = self.inventory_edit_state else {
            return Vec::new();
        };
        state
            .groups
            .iter()
            .filter(|group| {
                self.groups_subtab_target_group
                    .as_ref()
                    .map(|target| target != *group)
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    fn sync_groups_subtab_tree_selection(&mut self) {
        let entries = self.groups_subtab_tree_nodes();
        if entries.is_empty() {
            self.groups_subtab_tree_idx = 0;
            self.groups_subtab_target_group = None;
            return;
        }
        if self.groups_subtab_tree_idx >= entries.len() {
            self.groups_subtab_tree_idx = entries.len() - 1;
        }
        self.groups_subtab_target_group = entries[self.groups_subtab_tree_idx].0.clone();
    }

    fn toggle_groups_subtab_attachment(&mut self) {
        match self.groups_subtab_focus {
            GroupsFocus::Tree => {
                self.status_line =
                    String::from("Move focus to Groups or Hosts to toggle attachment");
            }
            GroupsFocus::Groups => {
                let candidates = self.groups_subtab_candidate_groups();
                let Some(child) = candidates.get(self.groups_subtab_group_idx).cloned() else {
                    return;
                };
                let Some(ref mut state) = self.inventory_edit_state else {
                    return;
                };
                if let Some(parent) = self.groups_subtab_target_group.clone() {
                    if child == parent {
                        self.status_line = String::from("Group cannot be child of itself");
                        return;
                    }
                    let linked = state
                        .group_children
                        .get(&parent)
                        .map(|c| c.contains(&child))
                        .unwrap_or(false);
                    if linked {
                        if let Some(children) = state.group_children.get_mut(&parent) {
                            children.retain(|g| g != &child);
                        }
                        state.dirty = true;
                        self.status_line = format!("Unlinked {child} from {parent}");
                    } else {
                        let mut prospective = state.group_children.clone();
                        for children in prospective.values_mut() {
                            children.retain(|g| g != &child);
                        }
                        prospective
                            .entry(parent.clone())
                            .or_default()
                            .push(child.clone());
                        if group_children_has_cycle(&prospective) {
                            self.status_line = String::from("Link would create a cycle");
                            return;
                        }
                        for children in state.group_children.values_mut() {
                            children.retain(|g| g != &child);
                        }
                        state
                            .group_children
                            .entry(parent.clone())
                            .or_default()
                            .push(child.clone());
                        state.dirty = true;
                        self.status_line = format!("Linked {child} under {parent}");
                    }
                } else {
                    let mut removed = false;
                    for children in state.group_children.values_mut() {
                        let before = children.len();
                        children.retain(|g| g != &child);
                        removed |= children.len() != before;
                    }
                    if removed {
                        state.dirty = true;
                        self.status_line = format!("Moved {child} to root under all");
                    } else {
                        self.status_line = format!("{child} is already at root");
                    }
                }
            }
            GroupsFocus::Hosts => {
                let Some(ref mut state) = self.inventory_edit_state else {
                    return;
                };
                let Some(host) = state.hosts.get(self.groups_subtab_host_idx).cloned() else {
                    return;
                };
                if let Some(target) = self.groups_subtab_target_group.clone() {
                    let entry = state.assignments.entry(target.clone()).or_default();
                    if let Some(pos) = entry.iter().position(|h| h == &host) {
                        entry.remove(pos);
                        state.dirty = true;
                        self.status_line = format!("Removed {host} from {target}");
                    } else {
                        entry.push(host.clone());
                        state.dirty = true;
                        self.status_line = format!("Added {host} to {target}");
                    }
                } else {
                    let mut removed = false;
                    for assigned in state.assignments.values_mut() {
                        let before = assigned.len();
                        assigned.retain(|h| h != &host);
                        removed |= assigned.len() != before;
                    }
                    if removed {
                        state.dirty = true;
                        self.status_line = format!("Moved {host} to ungrouped (all)");
                    } else {
                        self.status_line = format!("{host} is already ungrouped");
                    }
                }
            }
        }
    }

    fn detach_groups_subtab_item(&mut self) {
        match self.groups_subtab_focus {
            GroupsFocus::Tree => {
                self.status_line = String::from("Move focus to Groups or Hosts to detach");
            }
            GroupsFocus::Groups => {
                let candidates = self.groups_subtab_candidate_groups();
                let Some(child) = candidates.get(self.groups_subtab_group_idx).cloned() else {
                    return;
                };
                let Some(ref mut state) = self.inventory_edit_state else {
                    return;
                };
                if let Some(parent) = self.groups_subtab_target_group.clone() {
                    if let Some(children) = state.group_children.get_mut(&parent) {
                        let before = children.len();
                        children.retain(|g| g != &child);
                        if children.len() != before {
                            state.dirty = true;
                            self.status_line = format!("Unlinked {child} from {parent}");
                        }
                    }
                } else {
                    let mut removed = false;
                    for children in state.group_children.values_mut() {
                        let before = children.len();
                        children.retain(|g| g != &child);
                        removed |= children.len() != before;
                    }
                    if removed {
                        state.dirty = true;
                        self.status_line = format!("Moved {child} to root");
                    }
                }
            }
            GroupsFocus::Hosts => {
                let Some(ref mut state) = self.inventory_edit_state else {
                    return;
                };
                let Some(host) = state.hosts.get(self.groups_subtab_host_idx).cloned() else {
                    return;
                };
                if let Some(target) = self.groups_subtab_target_group.clone() {
                    if let Some(entry) = state.assignments.get_mut(&target) {
                        let before = entry.len();
                        entry.retain(|h| h != &host);
                        if entry.len() != before {
                            state.dirty = true;
                            self.status_line = format!("Removed {host} from {target}");
                        }
                    }
                } else {
                    let mut removed = false;
                    for assigned in state.assignments.values_mut() {
                        let before = assigned.len();
                        assigned.retain(|h| h != &host);
                        removed |= assigned.len() != before;
                    }
                    if removed {
                        state.dirty = true;
                        self.status_line = format!("Moved {host} to ungrouped");
                    }
                }
            }
        }
    }

    fn delete_groups_subtab_item(&mut self) {
        match self.groups_subtab_focus {
            GroupsFocus::Tree => {}
            GroupsFocus::Groups => {
                let candidates = self.groups_subtab_candidate_groups();
                let Some(group) = candidates.get(self.groups_subtab_group_idx).cloned() else {
                    return;
                };
                let Some(ref mut state) = self.inventory_edit_state else {
                    return;
                };
                state.groups.retain(|g| g != &group);
                state.assignments.remove(&group);
                state.group_children.remove(&group);
                for children in state.group_children.values_mut() {
                    children.retain(|g| g != &group);
                }
                state.dirty = true;
                let len = self.groups_subtab_candidate_groups().len();
                if self.groups_subtab_group_idx >= len && len > 0 {
                    self.groups_subtab_group_idx = len - 1;
                }
                self.status_line = format!("Deleted group {group}");
            }
            GroupsFocus::Hosts => {
                let Some(ref mut state) = self.inventory_edit_state else {
                    return;
                };
                let Some(host) = state.hosts.get(self.groups_subtab_host_idx).cloned() else {
                    return;
                };
                state.hosts.retain(|h| h != &host);
                state.host_vars.remove(&host);
                for assigned in state.assignments.values_mut() {
                    assigned.retain(|h| h != &host);
                }
                state.dirty = true;
                if self.groups_subtab_host_idx >= state.hosts.len() && !state.hosts.is_empty() {
                    self.groups_subtab_host_idx = state.hosts.len() - 1;
                }
                self.status_line = format!("Deleted host {host}");
            }
        }
    }

    fn save_inventory_edit_state(&mut self) {
        let Some(ref state) = self.inventory_edit_state else {
            self.status_line = String::from("No inventory loaded for editing");
            return;
        };
        let yaml = render_inventory_yaml_with_vars(
            &state.hosts,
            &state.groups,
            &state.assignments,
            &state.group_children,
            &state.host_vars,
        );
        if let Err(err) = fs::write(&state.path, &yaml) {
            self.status_line = format!("Failed to save inventory: {err}");
            return;
        }
        if let Some(ref mut state) = self.inventory_edit_state {
            state.dirty = false;
        }
        let path_display = self
            .inventory_edit_state
            .as_ref()
            .map(|s| display_path(self.active_project_root(), &s.path))
            .unwrap_or_default();
        self.refresh_project();
        self.status_line = format!("Inventory saved: {path_display}");
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

        let inventories_dir = self.active_project_root().join("inventory");
        if let Err(err) = fs::create_dir_all(&inventories_dir) {
            self.status_line = format!("Failed to create inventories directory: {err}");
            return;
        }
        let new_path = inventories_dir.join(&filename);
        if new_path.exists() {
            self.status_line = format!(
                "Inventory already exists: {}",
                display_path(self.active_project_root(), &new_path)
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
        self.status_line = format!(
            "Created inventory {}",
            display_path(self.active_project_root(), &new_path)
        );
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

        let display = display_path(self.active_project_root(), &path);
        self.pending_inventory_delete = Some(path);
        self.status_line = format!("Press Shift+D again to delete {display}");
    }

    fn delete_inventory_file(&mut self, path: PathBuf) {
        let inventories_root = self.active_project_root().join("inventory");
        if path.strip_prefix(&inventories_root).is_err() {
            self.status_line = String::from("Refusing to delete outside ./inventory");
            return;
        }

        match fs::remove_file(&path) {
            Ok(_) => {
                let display = display_path(self.active_project_root(), &path);
                self.refresh_project();
                self.status_line = format!("Deleted inventory {display}");
            }
            Err(err) => {
                self.status_line = format!("Failed to delete inventory: {err}");
            }
        }
    }

    fn restore_history(&mut self) {
        match load_runs(self.active_project_root()) {
            Ok(runs) => {
                if runs.is_empty() {
                    self.runs.clear();
                    self.run_idx = 0;
                    self.next_run_id = 1;
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
        if let Err(err) = save_run(self.active_project_root(), run) {
            self.status_line = format!("history save failed: {err}");
        }
    }

    fn persist_all_runs(&mut self) {
        if self.runs.is_empty() {
            return;
        }
        let root = self.active_project_root().to_path_buf();
        for run in &self.runs {
            if let Err(err) = save_run(&root, run) {
                self.status_line = format!("history save failed: {err}");
                return;
            }
        }
    }

    fn request_quit(&mut self) {
        self.persist_all_runs();
        self.should_quit = true;
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
        self.runtime_candidates = discover_runtime_candidates(
            self.active_project_root(),
            Some(&self.run_options.ansible_bin),
        );
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
        spawn_bootstrap_managed_runtime(self.active_project_root().to_path_buf(), tx.clone());
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
        match load_playbook_settings(self.active_project_root()) {
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
        if let Err(err) =
            save_playbook_settings(self.active_project_root(), &self.playbook_settings)
        {
            self.status_line = format!("playbook settings save failed: {err}");
        }
    }

    fn restore_job_templates(&mut self) {
        match load_job_templates(self.active_project_root()) {
            Ok(templates) => {
                self.job_templates = templates;
                self.template_idx = 0;
                self.pending_template_delete = None;
                self.sync_run_selection_to_selected_template();
            }
            Err(err) => {
                self.status_line = format!("job templates load failed: {err}");
            }
        }
    }

    fn persist_job_templates(&mut self) {
        if let Err(err) = save_job_templates(self.active_project_root(), &self.job_templates) {
            self.status_line = format!("job templates save failed: {err}");
        }
    }

    fn restore_ansible_cfg_settings(&mut self) {
        match load_ansible_cfg_settings(self.active_project_root()) {
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
        if let Err(err) = save_ansible_cfg_settings(self.active_project_root(), &self.ansible_cfg) {
            self.status_line = format!("ansible.cfg save failed: {err}");
        }
    }

    fn ensure_settings_for_playbooks(&mut self) {
        let project_root = self.active_project_root().to_path_buf();
        let keys = self
            .playbooks
            .iter()
            .map(|p| display_path(&project_root, p))
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
            ssh_private_key_file: None,
            ssh_private_key_inline: None,
        }
    }

    fn selected_playbook_key(&self) -> Option<String> {
        let project_root = self.active_project_root();
        self.playbooks
            .get(self.playbook_idx)
            .map(|p| display_path(project_root, p))
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
        self.status_line = String::from(
            "Playbook settings: j/k field, h/l or arrows adjust, Enter edit/save (inline key uses Ctrl+S to save)",
        );
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
        let project_root = self.active_project_root();
        self.inventories
            .iter()
            .find(|path| display_path(project_root, path) == inventory)
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
            .map(|path| display_path(self.active_project_root(), path))
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
            self.status_line = String::from("No inventories found under ./inventory");
            return;
        }
        let Some(playbook) = self.selected_playbook_key() else {
            self.status_line = String::from("No playbook selected");
            return;
        };

        let current_display = self
            .selected_inventory_display_for_current_playbook()
            .unwrap_or_else(|| display_path(self.active_project_root(), &self.inventories[0]));
        let current_idx = self
            .inventories
            .iter()
            .position(|path| display_path(self.active_project_root(), path) == current_display)
            .unwrap_or(self.inventory_idx.min(self.inventories.len() - 1));

        let next_idx = if delta.is_positive() {
            min(current_idx + 1, self.inventories.len() - 1)
        } else {
            current_idx.saturating_sub(1)
        };
        let next_inventory = display_path(self.active_project_root(), &self.inventories[next_idx]);
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

    // ── Templates ──────────────────────────────────────────────

    pub fn filtered_template_indices(&self) -> Vec<usize> {
        self.job_templates
            .iter()
            .enumerate()
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_template(&self) -> Option<&JobTemplate> {
        self.job_templates.get(self.template_idx)
    }

    pub fn run_indices_for_selected_template(&self) -> Vec<usize> {
        let Some(template) = self.selected_template() else {
            return Vec::new();
        };
        let tid = &template.id;
        self.runs
            .iter()
            .enumerate()
            .filter_map(|(idx, run)| {
                if run.template_id.as_deref() == Some(tid) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    fn sync_run_selection_to_selected_template(&mut self) {
        let indices = self.run_indices_for_selected_template();
        if indices.is_empty() {
            self.run_idx = self.runs.len();
            self.log_cursor = 0;
            self.log_anchor = None;
            return;
        }

        if !indices.contains(&self.run_idx) {
            self.run_idx = indices[0];
        }
        self.sync_log_cursor_to_selected_run();
    }

    fn move_template_run_selection(&mut self, delta: i32) {
        let indices = self.run_indices_for_selected_template();
        if indices.is_empty() {
            return;
        }
        let current_pos = indices.iter().position(|i| *i == self.run_idx).unwrap_or(0);
        let new_pos = if delta > 0 {
            min(current_pos + 1, indices.len() - 1)
        } else {
            current_pos.saturating_sub(1)
        };
        self.run_idx = indices[new_pos];
    }

    fn start_template_run(&mut self, tx: &UnboundedSender<Action>) {
        if self.template_editor_open {
            self.status_line = String::from("Close editor before running");
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
        let Some(template) = self.job_templates.get(self.template_idx).cloned() else {
            self.status_line = String::from("No template selected");
            return;
        };
        if template.playbook.trim().is_empty() {
            self.status_line = String::from("Template must have a playbook set");
            return;
        }
        let project_root = self.active_project_root().to_path_buf();
        let (inventories, playbooks, vars_files) = discover_project(&project_root);
        self.apply_discovered_project(inventories, playbooks, vars_files);

        let resolved = match self.resolve_template_run_context(&template, true) {
            Ok(resolved) => resolved,
            Err(err) => {
                self.status_line = err;
                return;
            }
        };

        let run_id = self.next_run_id;
        self.next_run_id += 1;

        let warning_suffix = if resolved.warnings.is_empty() {
            String::new()
        } else {
            format!(" | {}", resolved.warnings.join(" | "))
        };
        self.status_line = format!(
            "Starting template run #{run_id} ({}) [inv: {} via {} | ssh: {}]{}",
            template.name,
            resolved.inventory,
            resolved.inventory_source,
            resolved.ssh_key_source,
            warning_suffix
        );
        spawn_ansible_run(
            RunRequest {
                run_id,
                cwd: project_root,
                playbook: template.playbook.clone(),
                inventory: resolved.inventory,
                options: resolved.options,
                template_id: Some(template.id.clone()),
                environment: Some(self.active_project_name()),
            },
            tx.clone(),
        );
    }

    fn open_template_editor_new(&mut self) {
        self.template_editor_open = true;
        self.template_editor_editing_id = None;
        self.template_editor_field_idx = 0;
        self.template_editor_text_mode = false;
        self.template_editor_text_buffer.clear();
        self.template_editor_name = String::new();
        self.template_editor_playbook_idx = self
            .playbook_idx
            .min(self.playbooks.len().saturating_sub(1));
        self.template_editor_inventory_idx = self
            .inventory_idx
            .min(self.inventories.len().saturating_sub(1));
        self.template_editor_settings = self
            .playbooks
            .get(self.template_editor_playbook_idx)
            .map(|path| display_path(self.active_project_root(), path))
            .and_then(|key| self.playbook_settings.get(&key).cloned())
            .unwrap_or_else(|| self.default_settings());
        self.status_line = String::from("New template: fill in fields, Ctrl+S to save");
    }

    fn open_template_editor_edit(&mut self) {
        let Some(template) = self.job_templates.get(self.template_idx).cloned() else {
            self.status_line = String::from("No template selected");
            return;
        };
        self.template_editor_open = true;
        self.template_editor_editing_id = Some(template.id.clone());
        self.template_editor_field_idx = 0;
        self.template_editor_text_mode = false;
        self.template_editor_text_buffer.clear();
        self.template_editor_name = template.name.clone();
        self.template_editor_playbook_idx = self
            .playbooks
            .iter()
            .position(|p| display_path(self.active_project_root(), p) == template.playbook)
            .unwrap_or(0);
        self.template_editor_inventory_idx = self
            .inventories
            .iter()
            .position(|p| display_path(self.active_project_root(), p) == template.inventory)
            .unwrap_or(0);
        self.template_editor_settings = PlaybookSettings {
            check: template.check,
            diff: template.diff,
            become_enabled: template.become_enabled,
            verbosity: template.verbosity,
            forks: template.forks,
            timeout: template.timeout,
            limit: template.limit,
            tags: template.tags,
            extra_vars: template.extra_vars,
            extra_args: template.extra_args,
            ssh_private_key_file: template.ssh_private_key_file,
            ssh_private_key_inline: template.ssh_private_key_inline,
        };
        self.status_line = format!("Editing template: {}", template.name);
    }

    fn close_template_editor(&mut self) {
        self.template_editor_open = false;
        self.template_editor_text_mode = false;
        self.template_editor_text_buffer.clear();
        self.status_line = String::from("Template editor closed");
    }

    fn save_template_from_editor(&mut self) {
        let name = self.template_editor_name.trim().to_string();
        if name.is_empty() {
            self.status_line = String::from("Template name cannot be empty");
            return;
        }
        let playbook = self
            .playbooks
            .get(self.template_editor_playbook_idx)
            .map(|p| display_path(self.active_project_root(), p))
            .unwrap_or_default();
        let inventory = self
            .inventories
            .get(self.template_editor_inventory_idx)
            .map(|p| display_path(self.active_project_root(), p))
            .unwrap_or_default();
        let s = &self.template_editor_settings;
        if let Some(ref editing_id) = self.template_editor_editing_id.clone() {
            if let Some(t) = self.job_templates.iter_mut().find(|t| &t.id == editing_id) {
                t.name = name.clone();
                t.playbook = playbook;
                t.inventory = inventory;
                t.environment = None;
                t.check = s.check;
                t.diff = s.diff;
                t.become_enabled = s.become_enabled;
                t.verbosity = s.verbosity;
                t.forks = s.forks;
                t.timeout = s.timeout;
                t.limit = s.limit.clone();
                t.tags = s.tags.clone();
                t.extra_vars = s.extra_vars.clone();
                t.extra_args = s.extra_args.clone();
                t.ssh_private_key_file = s.ssh_private_key_file.clone();
                t.ssh_private_key_inline = s.ssh_private_key_inline.clone();
            }
        } else {
            let mut t = JobTemplate::new(&name);
            t.playbook = playbook;
            t.inventory = inventory;
            t.environment = None;
            t.check = s.check;
            t.diff = s.diff;
            t.become_enabled = s.become_enabled;
            t.verbosity = s.verbosity;
            t.forks = s.forks;
            t.timeout = s.timeout;
            t.limit = s.limit.clone();
            t.tags = s.tags.clone();
            t.extra_vars = s.extra_vars.clone();
            t.extra_args = s.extra_args.clone();
            t.ssh_private_key_file = s.ssh_private_key_file.clone();
            t.ssh_private_key_inline = s.ssh_private_key_inline.clone();
            self.job_templates.push(t);
            self.template_idx = self.job_templates.len() - 1;
        }

        self.persist_job_templates();
        self.sync_run_selection_to_selected_template();
        self.close_template_editor();
        self.status_line = format!("Template saved: {name}");
    }

    fn delete_selected_template(&mut self) {
        if self.job_templates.is_empty() {
            return;
        }
        let Some(template) = self.job_templates.get(self.template_idx) else {
            return;
        };
        if self.pending_template_delete.as_deref() == Some(&template.id) {
            let name = template.name.clone();
            self.job_templates.remove(self.template_idx);
            if self.template_idx >= self.job_templates.len() && self.template_idx > 0 {
                self.template_idx -= 1;
            }
            self.pending_template_delete = None;
            self.persist_job_templates();
            self.sync_run_selection_to_selected_template();
            self.status_line = format!("Template deleted: {name}");
        } else {
            self.pending_template_delete = Some(template.id.clone());
            self.status_line =
                format!("Press Shift+D again to confirm deleting: {}", template.name);
        }
    }

    fn confirm_template_editor(&mut self) {
        if !self.template_editor_open {
            return;
        }
        if self.template_editor_text_mode {
            if self.template_editor_is_multiline_field() {
                self.template_editor_text_buffer.push('\n');
            } else {
                self.commit_template_editor_text_edit();
            }
            return;
        }
        if self.template_editor_is_text_field() {
            self.begin_template_editor_text_edit();
            return;
        }
        self.toggle_template_editor_boolean();
    }

    pub fn template_editor_is_text_field(&self) -> bool {
        // Fields: 0=name, 1=playbook, 2=inventory,
        //         4=check, 5=diff, 6=become, 7=verbosity,
        //         8=forks, 9=timeout,
        //         10=limit, 11=tags, 12=extra_vars, 13=extra_args,
        //         14=ssh key file, 15=ssh key inline
        matches!(
            self.template_editor_field_idx,
            0 | 10 | 11 | 12 | 13 | 14 | 15
        )
    }

    pub fn template_editor_is_multiline_field(&self) -> bool {
        self.template_editor_field_idx == 15
    }

    fn handle_template_editor_char(&mut self, ch: char) {
        if self.template_editor_text_mode {
            self.template_editor_text_buffer.push(ch);
            return;
        }
        match ch {
            'j' => {
                self.template_editor_field_idx = min(
                    self.template_editor_field_idx + 1,
                    TEMPLATE_EDITOR_FIELD_COUNT - 1,
                );
            }
            'k' => {
                self.template_editor_field_idx = self.template_editor_field_idx.saturating_sub(1);
            }
            'h' => self.adjust_template_editor_field(-1),
            'l' => self.adjust_template_editor_field(1),
            ' ' => self.toggle_template_editor_boolean(),
            'e' => self.begin_template_editor_text_edit(),
            _ => {}
        }
    }

    fn adjust_template_editor_field(&mut self, delta: i8) {
        if self.template_editor_text_mode {
            return;
        }
        let s = &mut self.template_editor_settings;
        match self.template_editor_field_idx {
            1 => {
                // playbook picker
                if !self.playbooks.is_empty() {
                    if delta > 0 {
                        self.template_editor_playbook_idx = min(
                            self.template_editor_playbook_idx + 1,
                            self.playbooks.len() - 1,
                        );
                    } else {
                        self.template_editor_playbook_idx =
                            self.template_editor_playbook_idx.saturating_sub(1);
                    }
                }
            }
            2 => {
                // inventory picker
                if !self.inventories.is_empty() {
                    if delta > 0 {
                        self.template_editor_inventory_idx = min(
                            self.template_editor_inventory_idx + 1,
                            self.inventories.len() - 1,
                        );
                    } else {
                        self.template_editor_inventory_idx =
                            self.template_editor_inventory_idx.saturating_sub(1);
                    }
                }
            }
            4 => s.check = !s.check,
            5 => s.diff = !s.diff,
            6 => s.become_enabled = !s.become_enabled,
            7 => {
                let v = s.verbosity as i8 + delta;
                s.verbosity = max(0, min(4, v)) as u8;
            }
            8 => {
                s.forks = cycle_u16(
                    s.forks,
                    &[None, Some(5), Some(10), Some(20), Some(50)],
                    delta,
                );
            }
            9 => {
                s.timeout = cycle_u16(
                    s.timeout,
                    &[None, Some(10), Some(30), Some(60), Some(120)],
                    delta,
                );
            }
            _ => {}
        }
    }

    fn toggle_template_editor_boolean(&mut self) {
        if self.template_editor_text_mode {
            return;
        }
        let s = &mut self.template_editor_settings;
        match self.template_editor_field_idx {
            4 => s.check = !s.check,
            5 => s.diff = !s.diff,
            6 => s.become_enabled = !s.become_enabled,
            _ => {}
        }
    }

    fn begin_template_editor_text_edit(&mut self) {
        if self.template_editor_text_mode {
            return;
        }
        let s = &self.template_editor_settings;
        let current = match self.template_editor_field_idx {
            0 => self.template_editor_name.clone(),
            10 => s.limit.clone().unwrap_or_default(),
            11 => s.tags.clone().unwrap_or_default(),
            12 => s.extra_vars.clone().unwrap_or_default(),
            13 => s.extra_args.clone().unwrap_or_default(),
            14 => s.ssh_private_key_file.clone().unwrap_or_default(),
            15 => s.ssh_private_key_inline.clone().unwrap_or_default(),
            _ => return,
        };
        self.template_editor_text_mode = true;
        self.template_editor_text_buffer = current;
    }

    fn commit_template_editor_text_edit(&mut self) {
        if !self.template_editor_text_mode {
            return;
        }
        let buf = self.template_editor_text_buffer.clone();
        let s = &mut self.template_editor_settings;
        match self.template_editor_field_idx {
            0 => self.template_editor_name = buf.trim().to_string(),
            10 => s.limit = normalize_optional_text(buf),
            11 => s.tags = normalize_optional_text(buf),
            12 => s.extra_vars = normalize_optional_text(buf),
            13 => s.extra_args = normalize_optional_text(buf),
            14 => s.ssh_private_key_file = normalize_optional_text(buf),
            15 => s.ssh_private_key_inline = normalize_optional_multiline_text(buf),
            _ => {}
        }
        self.template_editor_text_mode = false;
        self.template_editor_text_buffer.clear();
    }

    fn cancel_template_editor_text_edit(&mut self) {
        self.template_editor_text_mode = false;
        self.template_editor_text_buffer.clear();
    }

    pub fn template_effective_context(
        &self,
        template: &JobTemplate,
    ) -> Result<TemplateEffectiveContext, String> {
        let resolved = self.resolve_template_run_context(template, false)?;
        Ok(TemplateEffectiveContext {
            inventory: resolved.inventory,
            inventory_source: resolved.inventory_source,
            vars_files: resolved.options.extra_vars_files,
            ssh_private_key_file: resolved.options.ssh_private_key_file,
            has_inline_ssh_key: resolved.options.ssh_private_key_inline.is_some(),
            ssh_key_source: resolved.ssh_key_source,
            warnings: resolved.warnings,
        })
    }

    fn resolve_template_run_context(
        &self,
        template: &JobTemplate,
        validate_paths: bool,
    ) -> Result<ResolvedTemplateRunContext, String> {
        let mut options = template.to_run_options(&self.run_options.ansible_bin);
        let inventory = normalize_run_path(template.inventory.trim());
        let inventory_source = String::from("template");
        let mut ssh_key_source = if options.ssh_private_key_inline.is_some() {
            String::from("template-inline")
        } else if options.ssh_private_key_file.is_some() {
            String::from("template")
        } else {
            String::from("unset")
        };
        let warnings = Vec::new();

        options.extra_vars_files.clear();
        if options.ssh_private_key_file.is_none() && options.ssh_private_key_inline.is_none() {
            if let Some(project) = self.projects.get(self.active_project_idx) {
                if project.ssh_private_key_file.is_some()
                    || project.ssh_private_key_inline.is_some()
                {
                    options.ssh_private_key_file = project
                        .ssh_private_key_file
                        .as_deref()
                        .map(normalize_run_path);
                    options.ssh_private_key_inline = project.ssh_private_key_inline.clone();
                    ssh_key_source = String::from("project");
                }
            }
        }

        if inventory.is_empty() {
            return Err(String::from("Template has no inventory set."));
        }
        if validate_paths {
            if !self.run_path_exists(&inventory) {
                return Err(format!("Inventory path does not exist: {inventory}"));
            }
            for vars_file in &options.extra_vars_files {
                if !self.run_path_exists(vars_file) {
                    return Err(format!("Vars file does not exist: {vars_file}"));
                }
            }
            if let Some(ref key_path) = options.ssh_private_key_file {
                if !self.run_path_exists(key_path) {
                    return Err(format!("SSH key file does not exist: {key_path}"));
                }
            }
        }

        Ok(ResolvedTemplateRunContext {
            inventory,
            options,
            inventory_source,
            ssh_key_source,
            warnings,
        })
    }

    fn run_path_exists(&self, raw: &str) -> bool {
        let normalized = normalize_run_path(raw);
        if normalized.is_empty() {
            return false;
        }
        let path = PathBuf::from(&normalized);
        if path.is_absolute() {
            path.exists()
        } else {
            self.active_project_root().join(path).exists()
        }
    }
}

fn normalize_optional_multiline_text(value: String) -> Option<String> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
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

fn normalize_run_path(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    if let Some(suffix) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(suffix)
                .to_string_lossy()
                .to_string();
        }
    }
    raw.to_string()
}

fn ensure_ansible_project_layout(root: &Path) -> std::io::Result<()> {
    for dir in ["inventory", "inventory/group_vars", "playbooks", "roles"] {
        fs::create_dir_all(root.join(dir))?;
    }

    let ansible_cfg = root.join("ansible.cfg");
    if !ansible_cfg.exists() {
        fs::write(
            ansible_cfg,
            "[defaults]\n\
             inventory = ./inventory\n\
             roles_path = ./roles\n\
             stdout_callback = yaml\n\
             \n\
             [ssh_connection]\n\
             pipelining = True\n",
        )?;
    }

    let inventory = root.join("inventory").join("inventory.yml");
    if !inventory.exists() {
        fs::write(
            inventory,
            "---\nall:\n  hosts:\n    localhost:\n      ansible_connection: local\n",
        )?;
    }

    let group_vars_all = root.join("inventory").join("group_vars").join("all.yml");
    if !group_vars_all.exists() {
        fs::write(group_vars_all, "---\n")?;
    }

    let playbook = root.join("playbooks").join("site.yml");
    if !playbook.exists() {
        fs::write(
            playbook,
            "---\n# Import playbooks here as they are added, e.g.:\n# - import_playbook: setup.yml\n",
        )?;
    }

    Ok(())
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
    host_vars: BTreeMap<String, HostVars>,
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
    let mut current_host_name: Option<String> = None;

    let mut hosts = Vec::new();
    let mut groups = Vec::new();
    let mut assignments: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut group_children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut host_vars: BTreeMap<String, HostVars> = BTreeMap::new();

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
            current_host_name = None;
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
                current_host_name = None;
            }
            4 => {
                current_host_name = None;
                if in_all_hosts {
                    if let Some(host) = parse_yaml_mapping_key(trimmed) {
                        if host != "hosts" {
                            push_unique(&mut hosts, host.clone());
                            current_host_name = Some(host);
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
                if in_all_hosts {
                    if let Some(ref host_name) = current_host_name {
                        if let Some((key, value)) = trimmed.split_once(':') {
                            let key = key.trim();
                            let value = value.trim();
                            let vars = host_vars.entry(host_name.clone()).or_default();
                            match key {
                                "ansible_host" => vars.ansible_host = value.to_string(),
                                "ansible_user" => vars.ansible_user = value.to_string(),
                                "ansible_port" => {
                                    vars.ansible_port = value.parse::<u16>().ok();
                                }
                                "ansible_connection" => {
                                    vars.ansible_connection = value.to_string();
                                }
                                _ => {
                                    if !key.is_empty() {
                                        vars.custom_vars.push((key.to_string(), value.to_string()));
                                    }
                                }
                            }
                        }
                    }
                } else if in_children && current_group.is_some() {
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
        host_vars,
    })
}

fn discover_project(cwd: &Path) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let mut inventories = Vec::new();
    let mut playbooks = Vec::new();
    let mut vars_files = Vec::new();

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
            .unwrap_or_default()
            .to_ascii_lowercase();

        if is_inventory_file(cwd, path, &ext) {
            inventories.push(path.to_path_buf());
            continue;
        }

        let in_playbooks_dir = parent == Some("playbooks");
        let in_project_root = path.parent().map(|p| p == cwd).unwrap_or(false);
        if (in_playbooks_dir || in_project_root) && is_playbook_ext(&ext) {
            playbooks.push(path.to_path_buf());
            continue;
        }

        if is_vars_file(cwd, path, &ext) {
            vars_files.push(path.to_path_buf());
        }
    }

    inventories.sort();
    inventories.dedup();
    playbooks.sort();
    playbooks.dedup();
    vars_files.sort();
    vars_files.dedup();

    (inventories, playbooks, vars_files)
}

fn is_inventory_ext(ext: &str) -> bool {
    matches!(ext, "yml" | "yaml" | "ini")
}

fn is_playbook_ext(ext: &str) -> bool {
    matches!(ext, "yml" | "yaml")
}

fn is_vars_ext(ext: &str) -> bool {
    matches!(ext, "yml" | "yaml" | "json")
}

fn is_vars_file(cwd: &Path, path: &Path, ext: &str) -> bool {
    if !is_vars_ext(ext) {
        return false;
    }
    if path_contains_dir_under(path, cwd, "group_vars")
        || path_contains_dir_under(path, cwd, "host_vars")
        || path_contains_dir_under(path, cwd, "vars")
    {
        return true;
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    file_name.ends_with(".vars.yml")
        || file_name.ends_with(".vars.yaml")
        || file_name.ends_with(".vars.json")
}

fn is_inventory_file(cwd: &Path, path: &Path, ext: &str) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let in_group_vars = path_contains_dir_under(path, cwd, "group_vars");
    let in_host_vars = path_contains_dir_under(path, cwd, "host_vars");
    if in_group_vars || in_host_vars {
        return false;
    }
    let in_project_root = path.parent().map(|p| p == cwd).unwrap_or(false);
    let in_inventories_tree = path_contains_dir_under(path, cwd, "inventory");
    let in_hosts_tree = path_contains_dir_under(path, cwd, "hosts");

    if in_inventories_tree || in_hosts_tree {
        return is_inventory_ext(ext) || (ext.is_empty() && file_name == "hosts");
    }

    if in_project_root {
        let looks_like_inventory_name = file_name == "hosts"
            || file_name.starts_with("hosts.")
            || file_name.starts_with("inventory.");
        if !looks_like_inventory_name {
            return false;
        }
        return is_inventory_ext(ext) || (ext.is_empty() && file_name == "hosts");
    }

    false
}

fn path_contains_dir_under(path: &Path, cwd: &Path, dir_name: &str) -> bool {
    let Ok(rel) = path.strip_prefix(cwd) else {
        return false;
    };
    rel.components()
        .any(|component| component.as_os_str() == dir_name)
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

fn render_inventory_yaml_with_vars(
    hosts: &[String],
    groups: &[String],
    group_hosts: &BTreeMap<String, Vec<String>>,
    group_children: &BTreeMap<String, Vec<String>>,
    host_vars: &BTreeMap<String, HostVars>,
) -> String {
    let mut out = String::from("all:\n");

    let has_any_vars = hosts
        .iter()
        .any(|h| host_vars.get(h).map(|v| !v.is_empty()).unwrap_or(false));

    let mut assigned_hosts = Vec::new();
    for host in group_hosts.values().flatten() {
        if !assigned_hosts.contains(host) {
            assigned_hosts.push(host.clone());
        }
    }

    let hosts_with_vars: Vec<&String> = hosts
        .iter()
        .filter(|h| {
            host_vars.get(*h).map(|v| !v.is_empty()).unwrap_or(false) && assigned_hosts.contains(*h)
        })
        .collect();

    let unassigned_hosts: Vec<&String> = hosts
        .iter()
        .filter(|host| !assigned_hosts.contains(*host))
        .collect();

    let need_all_hosts_section = !unassigned_hosts.is_empty() || !hosts_with_vars.is_empty();

    if need_all_hosts_section || (has_any_vars && !hosts.is_empty()) {
        out.push_str("  hosts:\n");
        let mut written = HashSet::new();
        for host in hosts {
            let has_vars = host_vars.get(host).map(|v| !v.is_empty()).unwrap_or(false);
            let is_unassigned = !assigned_hosts.contains(host);
            if !has_vars && !is_unassigned {
                continue;
            }
            if !written.insert(host.clone()) {
                continue;
            }
            if has_vars {
                let vars = host_vars.get(host).unwrap();
                out.push_str(&format!("    {host}:\n"));
                for (key, value) in vars.to_yaml_mapping() {
                    out.push_str(&format!("      {key}: {value}\n"));
                }
            } else {
                out.push_str(&format!("    {host}: {{}}\n"));
            }
        }
    }

    if !groups.is_empty() {
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
    }
    out
}

fn compute_tree_nodes(
    groups: &[String],
    group_children: &BTreeMap<String, Vec<String>>,
) -> Vec<(Option<String>, usize)> {
    fn push_node(
        node: &str,
        depth: usize,
        known: &HashSet<String>,
        group_children: &BTreeMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        out: &mut Vec<(Option<String>, usize)>,
    ) {
        if !known.contains(node) || !visited.insert(node.to_string()) {
            return;
        }
        out.push((Some(node.to_string()), depth));
        if let Some(children) = group_children.get(node) {
            for child in children {
                push_node(child, depth + 1, known, group_children, visited, out);
            }
        }
    }

    let mut out = vec![(None, 0)];
    if groups.is_empty() {
        return out;
    }

    let known: HashSet<String> = groups.iter().cloned().collect();
    let mut parent_of = BTreeMap::new();
    for parent in groups {
        if let Some(children) = group_children.get(parent) {
            for child in children {
                if known.contains(child) {
                    parent_of
                        .entry(child.clone())
                        .or_insert_with(|| parent.clone());
                }
            }
        }
    }

    let roots: Vec<String> = groups
        .iter()
        .filter(|g| !parent_of.contains_key(*g))
        .cloned()
        .collect();

    let mut visited = HashSet::new();
    for root in roots {
        push_node(&root, 1, &known, group_children, &mut visited, &mut out);
    }
    for group in groups {
        if visited.insert(group.clone()) {
            out.push((Some(group.clone()), 1));
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

use crate::background::{
    append_background_output, clear_background_cancel, clear_background_output,
    list_background_output, load_background_run, record_background_run, request_background_cancel,
    BackgroundRun,
};
use crate::checkpoints::{create_checkpoint, latest_checkpoint, restore_checkpoint};
use crate::cockpit::{append_cockpit_event, load_cockpit_journal};
use crate::compatibility::{render_agent_command_catalog, render_agent_compatibility};
use crate::config::{ArgusCodeConfig, CONFIG_PATH, SMOKE_EVAL_PATH};
use crate::diff::load_diff_preview;
use crate::eval_dashboard::load_eval_dashboard;
use crate::eval_runner::{run_eval_suite, EvalRunOutput};
use crate::harness::{run_task_through_harness, HarnessRunOutput};
use crate::launch::{load_launch_checklist, render_launch_checklist};
use crate::memory::{append_lesson, load_memory_preview};
use crate::plans::{complete_current_step, create_plan, load_plan_status, queue_next_step};
use crate::project::{detect_project, init_project, ProjectProfile};
use crate::repair::{build_harness_repair_task, build_repair_task};
use crate::repo_map::load_repo_map;
use crate::review::{load_change_review, record_review_decision};
use crate::route_runner::{run_task_through_route, RouteRunOutput};
use crate::sessions::{list_sessions, SessionRecord};
use crate::tasks::{latest_resumable_task, list_tasks, queue_task, update_task_status, TaskRecord};
use crate::trace_view::{load_trace_preview, TracePreview};
use crate::verify::run_configured_verify;
use crate::workflow::load_workflow_status;
use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchPane {
    Project,
    Session,
    Trace,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchMode {
    Normal,
    CommandPalette,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteAction {
    RunLatestTask,
    StopBackgroundRun,
    Verify,
    Memory,
    History,
    RefreshDiff,
    RefreshFlow,
    SmokeEval,
    NewTask,
    CommandGuide,
}

#[derive(Debug, Clone, Copy)]
struct PaletteItem {
    action: PaletteAction,
    label: &'static str,
    detail: &'static str,
}

impl PaletteItem {
    fn matches_query(&self, query: &str) -> bool {
        let label = self.label.to_ascii_lowercase();
        let detail = self.detail.to_ascii_lowercase();
        query
            .split_whitespace()
            .all(|term| label.contains(term) || detail.contains(term))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlashCommandHint {
    command: &'static str,
    detail: &'static str,
}

const SLASH_COMMAND_HINTS: &[SlashCommandHint] = &[
    SlashCommandHint {
        command: "/verify",
        detail: "Run verification gate",
    },
    SlashCommandHint {
        command: "/check",
        detail: "Run verification gate",
    },
    SlashCommandHint {
        command: "/test",
        detail: "Run verification gate",
    },
    SlashCommandHint {
        command: "/ask",
        detail: "Queue a task from a familiar prompt",
    },
    SlashCommandHint {
        command: "/add",
        detail: "Queue a task",
    },
    SlashCommandHint {
        command: "/code",
        detail: "Queue a coding task",
    },
    SlashCommandHint {
        command: "/edit",
        detail: "Queue an edit task",
    },
    SlashCommandHint {
        command: "/fix",
        detail: "Queue a fix task",
    },
    SlashCommandHint {
        command: "/implement",
        detail: "Queue an implementation task",
    },
    SlashCommandHint {
        command: "/prompt",
        detail: "Queue a prompt-style coding task",
    },
    SlashCommandHint {
        command: "/run",
        detail: "Run latest queued task",
    },
    SlashCommandHint {
        command: "/resume",
        detail: "Run latest queued task",
    },
    SlashCommandHint {
        command: "/continue",
        detail: "Run latest queued task",
    },
    SlashCommandHint {
        command: "/stop",
        detail: "Stop active background run",
    },
    SlashCommandHint {
        command: "/cancel-run",
        detail: "Stop active background run",
    },
    SlashCommandHint {
        command: "/interrupt",
        detail: "Stop active background run",
    },
    SlashCommandHint {
        command: "/logs",
        detail: "Open terminal output",
    },
    SlashCommandHint {
        command: "/terminal",
        detail: "Open terminal output",
    },
    SlashCommandHint {
        command: "/output",
        detail: "Open terminal output",
    },
    SlashCommandHint {
        command: "/trace",
        detail: "Open trace timeline",
    },
    SlashCommandHint {
        command: "/timeline",
        detail: "Open trace timeline",
    },
    SlashCommandHint {
        command: "/commands",
        detail: "Search familiar agent command mappings",
    },
    SlashCommandHint {
        command: "/cheatsheet",
        detail: "Search familiar agent command mappings",
    },
    SlashCommandHint {
        command: "/migrate",
        detail: "Search familiar agent command mappings",
    },
    SlashCommandHint {
        command: "/route-run",
        detail: "Route latest task through cheap/strong models",
    },
    SlashCommandHint {
        command: "/map",
        detail: "Refresh repo map",
    },
    SlashCommandHint {
        command: "/eval",
        detail: "Refresh eval dashboard",
    },
    SlashCommandHint {
        command: "/evals",
        detail: "Refresh eval dashboard",
    },
    SlashCommandHint {
        command: "/eval-run",
        detail: "Run smoke eval or a suite path",
    },
    SlashCommandHint {
        command: "/doctor",
        detail: "Show agent compatibility",
    },
    SlashCommandHint {
        command: "/health",
        detail: "Show agent compatibility",
    },
    SlashCommandHint {
        command: "/compat",
        detail: "Show agent compatibility",
    },
    SlashCommandHint {
        command: "/tasks",
        detail: "Show task queue",
    },
    SlashCommandHint {
        command: "/cancel",
        detail: "Cancel a task by id",
    },
    SlashCommandHint {
        command: "/retry",
        detail: "Requeue a task by id",
    },
    SlashCommandHint {
        command: "/flow",
        detail: "Refresh workflow status",
    },
    SlashCommandHint {
        command: "/status",
        detail: "Refresh workflow status",
    },
    SlashCommandHint {
        command: "/plan",
        detail: "Create a durable work plan",
    },
    SlashCommandHint {
        command: "/next",
        detail: "Queue the next plan step",
    },
    SlashCommandHint {
        command: "/done",
        detail: "Complete current plan step with evidence",
    },
    SlashCommandHint {
        command: "/launch",
        detail: "Show launch readiness checklist",
    },
    SlashCommandHint {
        command: "/readiness",
        detail: "Show launch readiness checklist",
    },
    SlashCommandHint {
        command: "/diff",
        detail: "Refresh diff preview",
    },
    SlashCommandHint {
        command: "/review",
        detail: "Refresh change review",
    },
    SlashCommandHint {
        command: "/patch",
        detail: "Refresh patch review",
    },
    SlashCommandHint {
        command: "/accept",
        detail: "Record accepted review decision",
    },
    SlashCommandHint {
        command: "/rework",
        detail: "Queue a review follow-up task",
    },
    SlashCommandHint {
        command: "/history",
        detail: "Open session history",
    },
    SlashCommandHint {
        command: "/memory",
        detail: "Refresh project memory preview",
    },
    SlashCommandHint {
        command: "/remember",
        detail: "Append a durable lesson",
    },
    SlashCommandHint {
        command: "/mcp",
        detail: "Set or show MCP server command",
    },
    SlashCommandHint {
        command: "/mcp-allow",
        detail: "Allow an MCP tool by name",
    },
    SlashCommandHint {
        command: "/checkpoint",
        detail: "Save a rollback snapshot",
    },
    SlashCommandHint {
        command: "/rollback",
        detail: "Restore latest or named checkpoint",
    },
    SlashCommandHint {
        command: "/model",
        detail: "Set or show current model",
    },
    SlashCommandHint {
        command: "/provider",
        detail: "Set or show provider profile",
    },
    SlashCommandHint {
        command: "/sandbox",
        detail: "Set or show sandbox profile",
    },
    SlashCommandHint {
        command: "/approval",
        detail: "Set or show approval profile",
    },
    SlashCommandHint {
        command: "/clear",
        detail: "Clear terminal output",
    },
    SlashCommandHint {
        command: "/help",
        detail: "Open help",
    },
    SlashCommandHint {
        command: "/new",
        detail: "Focus new task input",
    },
];

const PALETTE_ITEMS: &[PaletteItem] = &[
    PaletteItem {
        action: PaletteAction::RunLatestTask,
        label: "Run latest queued task",
        detail: "Execute through the Argus harness and refresh trace output",
    },
    PaletteItem {
        action: PaletteAction::Verify,
        label: "Run verification gate",
        detail: "Focus Terminal / Verify and prepare configured commands",
    },
    PaletteItem {
        action: PaletteAction::Memory,
        label: "Open project memory",
        detail: "Focus Trace / Memory and show durable project context",
    },
    PaletteItem {
        action: PaletteAction::History,
        label: "Open session history",
        detail: "Focus Trace / Memory and review completed task sessions",
    },
    PaletteItem {
        action: PaletteAction::RefreshDiff,
        label: "Refresh diff preview",
        detail: "Reload git status and diff summary into the session panel",
    },
    PaletteItem {
        action: PaletteAction::RefreshFlow,
        label: "Refresh workflow status",
        detail: "Show the current queue, run, verify, review, and next action",
    },
    PaletteItem {
        action: PaletteAction::SmokeEval,
        label: "Open smoke eval",
        detail: "Prepare the generated .argus/evals/smoke.json suite",
    },
    PaletteItem {
        action: PaletteAction::NewTask,
        label: "New coding task",
        detail: "Focus the conversation input",
    },
    PaletteItem {
        action: PaletteAction::StopBackgroundRun,
        label: "Stop background run",
        detail: "Request cancellation for the active harness process",
    },
    PaletteItem {
        action: PaletteAction::CommandGuide,
        label: "Open agent command guide",
        detail: "Search familiar Claude Code, Codex, KimiCode, and MiMoCode commands",
    },
];

#[derive(Debug, Clone)]
pub struct WorkbenchApp {
    pub profile: ProjectProfile,
    pub config: ArgusCodeConfig,
    pub active_pane: WorkbenchPane,
    pub mode: WorkbenchMode,
    pub palette_selected: usize,
    pub palette_query: String,
    pub task_queue: Vec<TaskRecord>,
    pub session_history: Vec<SessionRecord>,
    pub diff_preview: String,
    pub change_review: String,
    pub workflow_status: String,
    pub plan_status: String,
    pub trace_preview: TracePreview,
    pub repo_map: String,
    pub eval_dashboard: String,
    pub memory_preview: String,
    pub cockpit_journal: String,
    pub progress_summary: String,
    pub terminal_log: Vec<String>,
    pub latest_trace_path: Option<PathBuf>,
    pub input: String,
    pub status: String,
    background_run_seen: Option<String>,
    background_output_seen: usize,
}

struct WorkbenchLoadedData {
    task_queue: Vec<TaskRecord>,
    session_history: Vec<SessionRecord>,
    diff_preview: String,
    change_review: String,
    workflow_status: String,
    plan_status: String,
    trace_preview: TracePreview,
    repo_map: String,
    eval_dashboard: String,
    memory_preview: String,
    cockpit_journal: String,
}

impl WorkbenchApp {
    pub fn new(profile: ProjectProfile, config: ArgusCodeConfig) -> Self {
        Self::with_state(
            profile,
            config,
            WorkbenchLoadedData {
                task_queue: Vec::new(),
                session_history: Vec::new(),
                diff_preview: "(not loaded)".into(),
                change_review: "(not loaded)".into(),
                workflow_status: "(not loaded)".into(),
                plan_status: "(not loaded)".into(),
                trace_preview: TracePreview::empty(),
                repo_map: "(not loaded)".into(),
                eval_dashboard: "(not loaded)".into(),
                memory_preview: "(not loaded)".into(),
                cockpit_journal: "(not loaded)".into(),
            },
        )
    }

    pub fn load(profile: ProjectProfile, config: ArgusCodeConfig) -> Result<Self> {
        let task_queue = list_tasks(&profile.root)?;
        let session_history = list_sessions(&profile.root)?;
        let diff_preview = load_diff_preview(&profile.root)?;
        let change_review = load_change_review(&profile.root)?;
        let workflow_status = load_workflow_status(&profile.root, &config.verify.commands)?;
        let plan_status = load_plan_status(&profile.root)?;
        let latest_trace_path = session_history.last().map(|session| session.trace.clone());
        let trace_preview = load_trace_preview(&profile.root, latest_trace_path.as_deref());
        let repo_map = load_repo_map(&profile.root, &profile, &config)?;
        let eval_dashboard = load_eval_dashboard(&profile.root)?;
        let memory_preview = load_memory_preview(&profile.root, &config.memory)?;
        let cockpit_journal = load_cockpit_journal(&profile.root)?;
        Ok(Self::with_state(
            profile,
            config,
            WorkbenchLoadedData {
                task_queue,
                session_history,
                diff_preview,
                change_review,
                workflow_status,
                plan_status,
                trace_preview,
                repo_map,
                eval_dashboard,
                memory_preview,
                cockpit_journal,
            },
        ))
    }

    fn with_state(
        profile: ProjectProfile,
        config: ArgusCodeConfig,
        data: WorkbenchLoadedData,
    ) -> Self {
        let latest_trace_path = data
            .session_history
            .last()
            .map(|session| session.trace.clone());
        let mut app = Self {
            profile,
            config,
            active_pane: WorkbenchPane::Session,
            mode: WorkbenchMode::Normal,
            palette_selected: 0,
            palette_query: String::new(),
            task_queue: data.task_queue,
            session_history: data.session_history,
            diff_preview: data.diff_preview,
            change_review: data.change_review,
            workflow_status: data.workflow_status,
            plan_status: data.plan_status,
            trace_preview: data.trace_preview,
            repo_map: data.repo_map,
            eval_dashboard: data.eval_dashboard,
            memory_preview: data.memory_preview,
            cockpit_journal: data.cockpit_journal,
            progress_summary: String::new(),
            terminal_log: Vec::new(),
            latest_trace_path,
            input: String::new(),
            status: "Ready. Type a task, Tab switches panes, Ctrl+K opens command palette.".into(),
            background_run_seen: None,
            background_output_seen: 0,
        };
        app.refresh_progress_summary_silent();
        app
    }

    pub fn next_pane(&mut self) {
        self.active_pane = match self.active_pane {
            WorkbenchPane::Project => WorkbenchPane::Session,
            WorkbenchPane::Session => WorkbenchPane::Trace,
            WorkbenchPane::Trace => WorkbenchPane::Terminal,
            WorkbenchPane::Terminal => WorkbenchPane::Project,
        };
    }

    pub fn push_input(&mut self, c: char) {
        self.input.push(c);
    }

    pub fn pop_input(&mut self) {
        self.input.pop();
    }

    pub fn submit_input(&mut self) {
        let task = self.input.trim().to_string();
        if task.is_empty() {
            self.status = "Enter a task to start an ArgusCode session.".into();
        } else if task.starts_with('/') {
            self.input.clear();
            self.execute_slash_command(&task);
        } else {
            self.queue_task_text(&task);
        }
    }

    fn complete_slash_command(&mut self) -> bool {
        let suggestions = slash_command_suggestions(&self.input);
        let Some(first) = suggestions.first() else {
            return false;
        };
        self.input = format!("{} ", first.command);
        self.status = format!(
            "Completed slash command: {} - {}",
            first.command, first.detail
        );
        true
    }

    fn open_palette(&mut self) {
        self.mode = WorkbenchMode::CommandPalette;
        self.palette_selected = 0;
        self.palette_query.clear();
        self.status =
            "Command palette open. Type to search, Up/Down select, Enter run, Esc close.".into();
    }

    fn close_overlay(&mut self) {
        self.mode = WorkbenchMode::Normal;
        self.status = "Ready.".into();
    }

    fn palette_next(&mut self) {
        let len = filtered_palette_items(&self.palette_query).len();
        if len > 0 {
            self.palette_selected = (self.palette_selected + 1) % len;
        }
    }

    fn palette_prev(&mut self) {
        let len = filtered_palette_items(&self.palette_query).len();
        if len == 0 {
            return;
        }
        self.palette_selected = if self.palette_selected == 0 {
            len - 1
        } else {
            self.palette_selected - 1
        };
    }

    fn push_palette_query(&mut self, c: char) {
        self.palette_query.push(c);
        self.palette_selected = 0;
        self.status = format!("Command palette search: {}", self.palette_query);
    }

    fn pop_palette_query(&mut self) {
        self.palette_query.pop();
        self.palette_selected = 0;
        if self.palette_query.is_empty() {
            self.status =
                "Command palette open. Type to search, Up/Down select, Enter run, Esc close."
                    .into();
        } else {
            self.status = format!("Command palette search: {}", self.palette_query);
        }
    }

    fn execute_palette_action(&mut self) {
        let items = filtered_palette_items(&self.palette_query);
        let Some(item) = items.get(self.palette_selected).copied() else {
            self.status = format!("No command matches '{}'.", self.palette_query);
            return;
        };
        let action = item.action;
        self.mode = WorkbenchMode::Normal;
        match action {
            PaletteAction::RunLatestTask => {
                if let Err(err) = self.start_latest_task_background() {
                    self.active_pane = WorkbenchPane::Terminal;
                    self.status = format!("Could not start background run: {err}");
                    self.terminal_log.push(format!("error: {err}"));
                }
            }
            PaletteAction::StopBackgroundRun => self.stop_background_run(),
            PaletteAction::Verify => {
                if let Err(err) = self.run_verify_gate() {
                    self.status = format!("Verification failed: {err}");
                }
            }
            PaletteAction::Memory => {
                self.refresh_memory_preview("Memory opened.");
            }
            PaletteAction::History => {
                self.active_pane = WorkbenchPane::Trace;
                self.status = format!(
                    "Session history opened: {} run(s)",
                    self.session_history.len()
                );
            }
            PaletteAction::RefreshDiff => {
                self.refresh_diff_preview();
            }
            PaletteAction::RefreshFlow => {
                self.refresh_workflow_status();
            }
            PaletteAction::SmokeEval => {
                self.active_pane = WorkbenchPane::Trace;
                self.status = "Smoke eval ready: argus eval .argus/evals/smoke.json".into();
            }
            PaletteAction::NewTask => {
                self.active_pane = WorkbenchPane::Session;
                self.status = "New task ready. Type in the conversation input.".into();
            }
            PaletteAction::CommandGuide => self.show_agent_command_guide(None),
        }
    }

    fn execute_slash_command(&mut self, raw: &str) {
        let command = raw.split_whitespace().next().unwrap_or_default();
        if matches!(command, "/run" | "/resume" | "/continue") {
            if let Err(err) = self.start_latest_task_background() {
                self.active_pane = WorkbenchPane::Terminal;
                self.status = format!("Could not start background run: {err}");
                self.terminal_log.push(format!("error: {err}"));
            }
            return;
        }
        let mut task_runner = run_task_through_harness;
        let mut eval_runner = run_eval_suite;
        let mut route_runner = run_task_through_route;
        self.execute_slash_command_with(raw, &mut task_runner, &mut eval_runner, &mut route_runner);
    }

    fn execute_slash_command_with<TaskRunner, EvalRunner, RouteRunner>(
        &mut self,
        raw: &str,
        task_runner: &mut TaskRunner,
        eval_runner: &mut EvalRunner,
        route_runner: &mut RouteRunner,
    ) where
        TaskRunner: FnMut(&Path, &TaskRecord) -> Result<HarnessRunOutput>,
        EvalRunner: FnMut(&Path, &ArgusCodeConfig, &Path) -> Result<EvalRunOutput>,
        RouteRunner: FnMut(&Path, &TaskRecord, &str, &str) -> Result<RouteRunOutput>,
    {
        let mut parts = raw.split_whitespace();
        let command = parts.next().unwrap_or_default();
        let args = parts.collect::<Vec<_>>();
        match command {
            "/verify" | "/check" | "/test" => {
                if let Err(err) = self.run_verify_gate() {
                    if !self.status.contains("Repair task queued") {
                        self.status = format!("Verification failed: {err}");
                    }
                }
            }
            "/ask" | "/add" | "/code" | "/edit" | "/fix" | "/implement" | "/prompt" => {
                self.queue_task_text(&args.join(" "))
            }
            "/run" | "/resume" | "/continue" => {
                if let Err(err) = self.run_latest_task_with(task_runner) {
                    self.active_pane = WorkbenchPane::Terminal;
                    self.status = format!("Harness run failed: {err}");
                    self.terminal_log.push(format!("error: {err}"));
                }
            }
            "/diff" => self.refresh_diff_preview(),
            "/flow" | "/status" => self.refresh_workflow_status(),
            "/logs" | "/terminal" | "/output" => self.open_terminal_logs(),
            "/trace" | "/timeline" => self.open_trace_timeline(),
            "/commands" | "/cheatsheet" | "/migrate" => {
                self.show_agent_command_guide(Some(&args.join(" ")))
            }
            "/plan" => self.create_work_plan(&args.join(" ")),
            "/next" => self.queue_next_plan_step(),
            "/done" => self.complete_plan_step(&args.join(" ")),
            "/launch" | "/readiness" => self.show_launch_readiness(),
            "/doctor" | "/health" | "/compat" => self.show_agent_compatibility(),
            "/review" | "/patch" => self.refresh_change_review(),
            "/accept" => self.accept_change_review(&args.join(" ")),
            "/rework" => self.queue_rework_task(&args.join(" ")),
            "/map" => self.refresh_repo_map(),
            "/eval" | "/evals" => self.refresh_eval_dashboard(),
            "/stop" | "/cancel-run" | "/interrupt" => self.stop_background_run(),
            "/route-run" => {
                let (cheap, strong) = self.route_models_from_args(&args);
                if let Err(err) = self.run_latest_task_with_route(&cheap, &strong, route_runner) {
                    self.active_pane = WorkbenchPane::Terminal;
                    self.status = format!("Route run failed: {err}");
                    self.terminal_log.push(format!("error: {err}"));
                }
            }
            "/eval-run" => {
                let suite = args
                    .first()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(SMOKE_EVAL_PATH));
                if let Err(err) = self.run_eval_suite_with(suite, eval_runner) {
                    self.active_pane = WorkbenchPane::Terminal;
                    self.status = format!("Eval run failed: {err}");
                    self.terminal_log.push(format!("error: {err}"));
                }
            }
            "/tasks" => self.show_task_queue(),
            "/cancel" => self.update_task_status_from_command(
                args.first().copied(),
                "canceled",
                "Task canceled",
                "cancel",
            ),
            "/retry" => self.update_task_status_from_command(
                args.first().copied(),
                "queued",
                "Task requeued",
                "retry",
            ),
            "/history" => {
                self.active_pane = WorkbenchPane::Trace;
                self.status = format!(
                    "Session history opened: {} run(s)",
                    self.session_history.len()
                );
            }
            "/memory" => {
                self.refresh_memory_preview("Memory opened.");
            }
            "/remember" => self.remember_lesson(&args.join(" ")),
            "/mcp" => self.update_mcp_profile(&args),
            "/mcp-allow" => self.add_mcp_allow(args.first().copied()),
            "/checkpoint" => self.save_checkpoint(&args.join(" ")),
            "/rollback" => self.rollback_checkpoint(args.first().copied()),
            "/sandbox" => self.update_sandbox_profile(args.first().copied()),
            "/approval" => self.update_approval_profile(args.first().copied()),
            "/model" | "/provider" => {
                if command == "/model" {
                    self.update_model(args.first().copied());
                } else {
                    self.update_provider_profile(&args);
                }
            }
            "/clear" => {
                self.terminal_log.clear();
                self.status = "Terminal output cleared.".into();
            }
            "/help" => {
                self.mode = WorkbenchMode::Help;
                self.status = "Help open. Press ? or Esc to close.".into();
            }
            "/new" => {
                self.active_pane = WorkbenchPane::Session;
                self.status = "New task ready. Type in the conversation input.".into();
            }
            _ => {
                self.status = unknown_slash_command_status(command);
            }
        }
    }

    fn queue_task_text(&mut self, task: &str) {
        let task = task.trim();
        if task.is_empty() {
            self.status = "Enter a task to start an ArgusCode session.".into();
            return;
        }
        match queue_task(&self.profile.root, task) {
            Ok(record) => {
                self.status = format!("Queued task {}: {}", record.id, record.text);
                self.task_queue.push(record);
                self.record_cockpit_event(
                    "queue",
                    &format!("queued task: {task}"),
                    "/run or /route-run",
                );
                self.refresh_workflow_status_silent();
                self.input.clear();
            }
            Err(err) => {
                self.status = format!("Could not queue task: {err}");
            }
        }
    }

    fn create_work_plan(&mut self, goal: &str) {
        self.active_pane = WorkbenchPane::Session;
        match create_plan(&self.profile.root, goal) {
            Ok(plan) => {
                self.plan_status = load_plan_status(&self.profile.root)
                    .unwrap_or_else(|err| format!("Could not load plan: {err}"));
                self.record_cockpit_event("plan", &format!("created plan: {}", plan.goal), "/next");
                self.status = format!("Plan created: {}", plan.goal);
            }
            Err(err) => {
                self.status = format!("Could not create plan: {err}");
            }
        }
    }

    fn queue_next_plan_step(&mut self) {
        match queue_next_step(&self.profile.root) {
            Ok(Some(record)) => {
                self.task_queue.push(record.clone());
                self.plan_status = load_plan_status(&self.profile.root)
                    .unwrap_or_else(|err| format!("Could not load plan: {err}"));
                self.refresh_workflow_status_silent();
                self.record_cockpit_event(
                    "plan",
                    &format!("queued next step: {}", record.id),
                    "/run or /done <evidence>",
                );
                self.status = format!("Plan step queued: {}", record.id);
            }
            Ok(None) => {
                self.plan_status = load_plan_status(&self.profile.root)
                    .unwrap_or_else(|err| format!("Could not load plan: {err}"));
                self.status = "No pending plan step. Use /plan <goal> or /done <evidence>.".into();
            }
            Err(err) => {
                self.status = format!("Could not queue plan step: {err}");
            }
        }
    }

    fn complete_plan_step(&mut self, evidence: &str) {
        match complete_current_step(&self.profile.root, evidence) {
            Ok(plan) => {
                self.plan_status = load_plan_status(&self.profile.root)
                    .unwrap_or_else(|err| format!("Could not load plan: {err}"));
                self.record_cockpit_event(
                    "plan",
                    &format!("completed plan step for: {}", plan.goal),
                    "/next or /plan <goal>",
                );
                self.status = "Plan step completed.".into();
            }
            Err(err) => {
                self.status = format!("Could not complete plan step: {err}");
            }
        }
    }

    fn show_launch_readiness(&mut self) {
        self.active_pane = WorkbenchPane::Terminal;
        match load_launch_checklist(&self.profile.root) {
            Ok(checks) => {
                self.terminal_log = vec![render_launch_checklist(&checks)];
                let ready = checks
                    .iter()
                    .filter(|check| check.status == "ready")
                    .count();
                self.status = format!("Launch readiness: {ready}/{} ready", checks.len());
            }
            Err(err) => {
                self.terminal_log = vec![format!("Could not load launch checklist: {err}")];
                self.status = format!("Could not load launch checklist: {err}");
            }
        }
    }

    fn route_models_from_args(&self, args: &[&str]) -> (String, String) {
        let (default_cheap, default_strong) = self.default_route_models();
        match args {
            [] => (default_cheap, default_strong),
            [cheap] => ((*cheap).to_string(), default_strong),
            [cheap, strong, ..] => ((*cheap).to_string(), (*strong).to_string()),
        }
    }

    fn default_route_models(&self) -> (String, String) {
        let cheap = self.config.provider.default_model.clone();
        let strong = if self
            .config
            .provider
            .api_key_env
            .as_deref()
            .is_some_and(|env| env == "DEEPSEEK_API_KEY")
            || self
                .config
                .provider
                .base_url
                .as_deref()
                .is_some_and(|url| url.contains("deepseek"))
        {
            "deepseek-reasoner".to_string()
        } else if self.config.provider.default_provider == "openai" {
            "gpt-4o".to_string()
        } else {
            cheap.clone()
        };
        (cheap, strong)
    }

    fn refresh_memory_preview(&mut self, label: &str) {
        self.active_pane = WorkbenchPane::Trace;
        match load_memory_preview(&self.profile.root, &self.config.memory) {
            Ok(preview) => {
                self.memory_preview = preview;
                self.status = format!(
                    "{label} project={}, lessons={}",
                    self.config.memory.project, self.config.memory.lessons
                );
            }
            Err(err) => {
                self.status = format!("Could not load memory: {err}");
            }
        }
    }

    fn remember_lesson(&mut self, lesson: &str) {
        self.active_pane = WorkbenchPane::Trace;
        match append_lesson(&self.profile.root, &self.config.memory, lesson) {
            Ok(_) => {
                self.refresh_memory_preview("Lesson remembered.");
            }
            Err(err) => {
                self.status = format!("Could not remember lesson: {err}");
            }
        }
    }

    fn update_mcp_profile(&mut self, args: &[&str]) {
        if args.is_empty() {
            self.show_mcp_profile("MCP profile shown.");
            return;
        }
        if args[0] == "off" {
            self.config.mcp.command = None;
            self.config.mcp.allow.clear();
            self.persist_mcp_profile("MCP profile updated");
            return;
        }
        self.config.mcp.command = Some(args.join(" "));
        self.persist_mcp_profile("MCP profile updated");
    }

    fn add_mcp_allow(&mut self, tool: Option<&str>) {
        let Some(tool) = tool else {
            self.show_mcp_profile("MCP profile shown.");
            return;
        };
        if !self.config.mcp.allow.iter().any(|item| item == tool) {
            self.config.mcp.allow.push(tool.to_string());
        }
        self.persist_mcp_profile("MCP profile updated");
    }

    fn persist_mcp_profile(&mut self, label: &str) {
        match self.config.write(&self.profile.root) {
            Ok(_) => self.show_mcp_profile(label),
            Err(err) => {
                self.active_pane = WorkbenchPane::Terminal;
                self.status = format!("Could not write MCP profile: {err}");
            }
        }
    }

    fn show_mcp_profile(&mut self, status: &str) {
        self.active_pane = WorkbenchPane::Terminal;
        self.terminal_log = vec![format!(
            "mcp: {}",
            self.config.mcp.command.as_deref().unwrap_or("(off)")
        )];
        if self.config.mcp.allow.is_empty() {
            self.terminal_log.push("allow: (empty)".into());
        } else {
            self.terminal_log
                .push(format!("allow: {}", self.config.mcp.allow.join(", ")));
        }
        self.status = format!(
            "{status}: {}",
            self.config.mcp.command.as_deref().unwrap_or("(off)")
        );
    }

    fn save_checkpoint(&mut self, label: &str) {
        self.active_pane = WorkbenchPane::Terminal;
        match create_checkpoint(&self.profile.root, label) {
            Ok(record) => {
                self.terminal_log = vec![
                    format!("checkpoint: {}", record.id),
                    format!("label: {}", record.label),
                    format!("files: {}", record.file_count),
                ];
                self.record_cockpit_event(
                    "checkpoint",
                    &format!("saved {} ({} files)", record.id, record.file_count),
                    "/run, /route-run, or /rollback",
                );
                self.status = format!("Checkpoint saved: {}", record.id);
            }
            Err(err) => {
                self.status = format!("Could not save checkpoint: {err}");
                self.terminal_log.push(format!("error: {err}"));
            }
        }
    }

    fn rollback_checkpoint(&mut self, checkpoint_id: Option<&str>) {
        self.active_pane = WorkbenchPane::Terminal;
        let checkpoint = match checkpoint_id {
            Some(id) => restore_checkpoint(&self.profile.root, id),
            None => match latest_checkpoint(&self.profile.root) {
                Ok(Some(record)) => restore_checkpoint(&self.profile.root, &record.id),
                Ok(None) => {
                    self.status = "No checkpoint found.".into();
                    self.terminal_log = vec!["No checkpoint found.".into()];
                    return;
                }
                Err(err) => Err(err),
            },
        };
        match checkpoint {
            Ok(record) => {
                self.diff_preview = load_diff_preview(&self.profile.root)
                    .unwrap_or_else(|err| format!("Could not refresh diff: {err}"));
                self.change_review = load_change_review(&self.profile.root)
                    .unwrap_or_else(|err| format!("Could not refresh change review: {err}"));
                self.refresh_workflow_status_silent();
                self.repo_map = load_repo_map(&self.profile.root, &self.profile, &self.config)
                    .unwrap_or_else(|err| format!("Could not refresh repo map: {err}"));
                self.terminal_log = vec![
                    format!("rolled back: {}", record.id),
                    format!("label: {}", record.label),
                    format!("files: {}", record.file_count),
                ];
                self.record_cockpit_event(
                    "rollback",
                    &format!("restored {} ({} files)", record.id, record.file_count),
                    "/flow, then /verify",
                );
                self.status = format!("Rolled back to checkpoint {}", record.id);
            }
            Err(err) => {
                self.status = format!("Rollback failed: {err}");
                self.terminal_log.push(format!("error: {err}"));
            }
        }
    }

    fn update_sandbox_profile(&mut self, sandbox: Option<&str>) {
        let Some(sandbox) = sandbox else {
            self.show_security_profile("Security profile shown.");
            return;
        };
        if !matches!(sandbox, "workspace-write" | "read-only" | "trusted") {
            self.status = "Unknown sandbox. Try workspace-write, read-only, or trusted.".into();
            return;
        }
        self.config.security.sandbox = sandbox.to_string();
        self.persist_security_profile("Sandbox updated");
    }

    fn update_approval_profile(&mut self, approval: Option<&str>) {
        let Some(approval) = approval else {
            self.show_security_profile("Security profile shown.");
            return;
        };
        if !matches!(approval, "auto" | "ask") {
            self.status = "Unknown approval profile. Try auto or ask.".into();
            return;
        }
        self.config.security.approval = approval.to_string();
        self.persist_security_profile("Approval updated");
    }

    fn persist_security_profile(&mut self, label: &str) {
        match self.config.write(&self.profile.root) {
            Ok(_) => self.show_security_profile(label),
            Err(err) => {
                self.active_pane = WorkbenchPane::Terminal;
                self.status = format!("Could not write security profile: {err}");
            }
        }
    }

    fn show_security_profile(&mut self, status: &str) {
        self.active_pane = WorkbenchPane::Terminal;
        self.terminal_log = vec![
            format!("sandbox: {}", self.config.security.sandbox),
            format!("approval: {}", self.config.security.approval),
        ];
        self.status = format!(
            "{status}: sandbox={}, approval={}",
            self.config.security.sandbox, self.config.security.approval
        );
    }

    fn show_task_queue(&mut self) {
        self.active_pane = WorkbenchPane::Project;
        match list_tasks(&self.profile.root) {
            Ok(tasks) => {
                self.task_queue = tasks;
                self.terminal_log = if self.task_queue.is_empty() {
                    vec!["Task queue is empty.".into()]
                } else {
                    self.task_queue
                        .iter()
                        .rev()
                        .map(|task| format!("[{}] {}  {}", task.status, task.id, task.text))
                        .collect()
                };
                self.refresh_workflow_status_silent();
                self.status = format!("Task queue opened: {} task(s)", self.task_queue.len());
            }
            Err(err) => {
                self.status = format!("Could not read task queue: {err}");
            }
        }
    }

    fn refresh_repo_map(&mut self) {
        self.active_pane = WorkbenchPane::Project;
        match load_repo_map(&self.profile.root, &self.profile, &self.config) {
            Ok(map) => {
                self.repo_map = map;
                self.status = "Repo map refreshed.".into();
            }
            Err(err) => {
                self.status = format!("Could not refresh repo map: {err}");
            }
        }
    }

    fn refresh_eval_dashboard(&mut self) {
        self.active_pane = WorkbenchPane::Trace;
        match load_eval_dashboard(&self.profile.root) {
            Ok(dashboard) => {
                self.eval_dashboard = dashboard;
                self.status = "Eval dashboard refreshed.".into();
            }
            Err(err) => {
                self.status = format!("Could not refresh eval dashboard: {err}");
            }
        }
    }

    fn update_task_status_from_command(
        &mut self,
        task_id: Option<&str>,
        next_status: &str,
        label: &str,
        command: &str,
    ) {
        let Some(task_id) = task_id else {
            self.status = format!("Usage: /{command} <task-id>");
            return;
        };
        self.active_pane = WorkbenchPane::Project;
        match update_task_status(&self.profile.root, task_id, next_status) {
            Ok(Some(task)) => {
                match list_tasks(&self.profile.root) {
                    Ok(tasks) => self.task_queue = tasks,
                    Err(err) => {
                        self.status = format!("{label}, but queue refresh failed: {err}");
                        return;
                    }
                }
                self.refresh_workflow_status_silent();
                self.record_cockpit_event(
                    "queue",
                    &format!("{label}: {}", task.id),
                    "/run or /route-run",
                );
                self.status = format!("{label}: {}", task.id);
            }
            Ok(None) => {
                self.status = format!("Task not found: {task_id}");
            }
            Err(err) => {
                self.status = format!("Could not update task {task_id}: {err}");
            }
        }
    }

    fn update_provider_profile(&mut self, args: &[&str]) {
        match args.first().copied() {
            None => self.show_provider_profile("Provider profile shown."),
            Some("mock") => {
                self.config.provider.default_provider = "mock".into();
                self.config.provider.default_model =
                    args.get(1).copied().unwrap_or("mock").to_string();
                self.config.provider.base_url = None;
                self.config.provider.api_key_env = None;
                self.config.provider.routing = "manual".into();
                self.persist_provider_profile("Provider updated");
            }
            Some("openai") => {
                self.config.provider.default_provider = "openai".into();
                self.config.provider.default_model =
                    args.get(1).copied().unwrap_or("gpt-4o-mini").to_string();
                self.config.provider.base_url = None;
                self.config.provider.api_key_env = Some("OPENAI_API_KEY".into());
                self.config.provider.routing = "manual".into();
                self.persist_provider_profile("Provider updated");
            }
            Some("deepseek") => {
                self.config.provider.default_provider = "openai".into();
                self.config.provider.default_model =
                    args.get(1).copied().unwrap_or("deepseek-chat").to_string();
                self.config.provider.base_url = Some("https://api.deepseek.com".into());
                self.config.provider.api_key_env = Some("DEEPSEEK_API_KEY".into());
                self.config.provider.routing = "manual".into();
                self.persist_provider_profile("Provider updated");
            }
            Some("custom") => {
                let Some(provider) = args.get(1).copied() else {
                    self.status =
                        "Usage: /provider custom <provider> <model> [base-url] [api-key-env]"
                            .into();
                    return;
                };
                let Some(model) = args.get(2).copied() else {
                    self.status =
                        "Usage: /provider custom <provider> <model> [base-url] [api-key-env]"
                            .into();
                    return;
                };
                self.config.provider.default_provider = provider.to_string();
                self.config.provider.default_model = model.to_string();
                self.config.provider.base_url = args.get(3).map(|value| (*value).to_string());
                self.config.provider.api_key_env = args.get(4).map(|value| (*value).to_string());
                self.config.provider.routing = "manual".into();
                self.persist_provider_profile("Provider updated");
            }
            Some(other) => {
                self.status = format!(
                    "Unknown provider profile: {other}. Try mock, openai, deepseek, or custom."
                );
            }
        }
    }

    fn update_model(&mut self, model: Option<&str>) {
        let Some(model) = model else {
            self.show_provider_profile("Provider profile shown.");
            return;
        };
        self.config.provider.default_model = model.to_string();
        self.persist_provider_profile("Model updated");
    }

    fn persist_provider_profile(&mut self, label: &str) {
        match self.config.write(&self.profile.root) {
            Ok(_) => self.show_provider_profile(label),
            Err(err) => {
                self.active_pane = WorkbenchPane::Terminal;
                self.status = format!("Could not write provider profile: {err}");
            }
        }
    }

    fn show_provider_profile(&mut self, status: &str) {
        self.active_pane = WorkbenchPane::Terminal;
        self.terminal_log = vec![
            format!("provider: {}", self.config.provider.default_provider),
            format!("model: {}", self.config.provider.default_model),
            format!("routing: {}", self.config.provider.routing),
        ];
        if let Some(base_url) = &self.config.provider.base_url {
            self.terminal_log.push(format!("base_url: {base_url}"));
        }
        if let Some(api_key_env) = &self.config.provider.api_key_env {
            self.terminal_log
                .push(format!("api_key_env: {api_key_env}"));
        }
        self.status = format!(
            "{status}: {}/{}",
            self.config.provider.default_provider, self.config.provider.default_model
        );
    }

    fn refresh_diff_preview(&mut self) {
        self.active_pane = WorkbenchPane::Session;
        match load_diff_preview(&self.profile.root) {
            Ok(preview) => {
                self.diff_preview = preview;
                self.refresh_workflow_status_silent();
                self.status = "Diff preview refreshed.".into();
            }
            Err(err) => {
                self.status = format!("Could not refresh diff preview: {err}");
            }
        }
    }

    fn refresh_workflow_status(&mut self) {
        self.active_pane = WorkbenchPane::Session;
        match load_workflow_status(&self.profile.root, &self.config.verify.commands) {
            Ok(status) => {
                self.workflow_status = status;
                self.status = "Workflow status refreshed.".into();
            }
            Err(err) => {
                self.status = format!("Could not refresh workflow status: {err}");
            }
        }
    }

    fn open_terminal_logs(&mut self) {
        self.active_pane = WorkbenchPane::Terminal;
        self.status = "Terminal logs opened.".into();
    }

    fn open_trace_timeline(&mut self) {
        self.active_pane = WorkbenchPane::Trace;
        self.trace_preview =
            load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
        self.status = "Trace timeline opened.".into();
    }

    fn show_agent_compatibility(&mut self) {
        self.active_pane = WorkbenchPane::Terminal;
        self.terminal_log = vec![render_agent_compatibility(&self.profile.root)];
        self.status = "Agent compatibility opened.".into();
    }

    fn show_agent_command_guide(&mut self, query: Option<&str>) {
        self.active_pane = WorkbenchPane::Terminal;
        self.terminal_log = vec![render_agent_command_catalog(query)];
        self.status = match query.map(str::trim).filter(|query| !query.is_empty()) {
            Some(query) => format!("Agent command guide opened for '{query}'."),
            None => "Agent command guide opened.".into(),
        };
    }

    fn refresh_workflow_status_silent(&mut self) {
        if let Ok(status) = load_workflow_status(&self.profile.root, &self.config.verify.commands) {
            self.workflow_status = status;
        }
    }

    fn refresh_cockpit_journal_silent(&mut self) {
        if let Ok(journal) = load_cockpit_journal(&self.profile.root) {
            self.cockpit_journal = journal;
        }
    }

    fn record_cockpit_event(&mut self, phase: &str, detail: &str, next: &str) {
        if append_cockpit_event(&self.profile.root, phase, detail, next).is_ok() {
            self.refresh_cockpit_journal_silent();
        }
    }

    pub fn tick(&mut self) {
        self.refresh_cockpit_journal_silent();
        self.refresh_background_output_silent();
        let Ok(Some(run)) = load_background_run(&self.profile.root) else {
            self.refresh_progress_summary_silent();
            return;
        };
        self.refresh_background_trace_silent(&run);
        let marker = background_run_marker(&run);
        if self.background_run_seen.as_deref() == Some(marker.as_str()) {
            return;
        }
        self.background_run_seen = Some(marker);
        match run.status.as_str() {
            "running" => {
                self.status = format!("Background run active: {}", run.task_id);
            }
            "canceling" => {
                self.status = format!("Background run stopping: {}", run.task_id);
            }
            "done" => {
                self.refresh_after_background_run(&run);
                self.status = format!("Background run done: {}", run.task_id);
            }
            "canceled" => {
                self.refresh_after_background_run(&run);
                self.status = format!("Background run canceled: {}", run.task_id);
            }
            "failed" => {
                self.refresh_after_background_run(&run);
                self.status = if run.detail.contains("repair queued") {
                    format!("Background run failed. Repair task queued: {}", run.task_id)
                } else {
                    format!("Background run failed: {}", run.task_id)
                };
            }
            _ => {}
        }
        self.refresh_progress_summary_silent();
    }

    fn refresh_progress_summary_silent(&mut self) {
        self.progress_summary = build_progress_summary(
            &self.status,
            &self.trace_preview,
            &self.cockpit_journal,
            &self.terminal_log,
        );
    }

    fn refresh_background_trace_silent(&mut self, run: &BackgroundRun) {
        let Some(trace) = run.trace.clone() else {
            return;
        };
        if matches!(run.status.as_str(), "running" | "canceling") {
            self.latest_trace_path = Some(trace);
            self.trace_preview =
                load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
        }
    }

    fn refresh_background_output_silent(&mut self) {
        let Ok(records) = list_background_output(&self.profile.root) else {
            return;
        };
        let next_unread = if records.len() < self.background_output_seen {
            0
        } else {
            self.background_output_seen
        };
        for record in records.iter().skip(next_unread) {
            self.terminal_log
                .push(format!("[{}] {}", record.stream, record.text));
        }
        self.background_output_seen = records.len();
        const MAX_TERMINAL_LINES: usize = 120;
        if self.terminal_log.len() > MAX_TERMINAL_LINES {
            let drop_count = self.terminal_log.len() - MAX_TERMINAL_LINES;
            self.terminal_log.drain(..drop_count);
        }
    }

    fn refresh_after_background_run(&mut self, run: &BackgroundRun) {
        self.task_queue =
            list_tasks(&self.profile.root).unwrap_or_else(|_| self.task_queue.clone());
        self.session_history =
            list_sessions(&self.profile.root).unwrap_or_else(|_| self.session_history.clone());
        self.latest_trace_path = run.trace.clone().or_else(|| {
            self.session_history
                .last()
                .map(|session| session.trace.clone())
        });
        self.diff_preview = load_diff_preview(&self.profile.root)
            .unwrap_or_else(|err| format!("Could not refresh diff: {err}"));
        self.change_review = load_change_review(&self.profile.root)
            .unwrap_or_else(|err| format!("Could not refresh change review: {err}"));
        self.workflow_status =
            load_workflow_status(&self.profile.root, &self.config.verify.commands)
                .unwrap_or_else(|err| format!("Could not refresh workflow status: {err}"));
        self.trace_preview =
            load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
        self.refresh_cockpit_journal_silent();
        self.terminal_log.push(run.detail.clone());
    }

    fn refresh_change_review(&mut self) {
        self.active_pane = WorkbenchPane::Session;
        match load_change_review(&self.profile.root) {
            Ok(review) => {
                self.change_review = review;
                self.refresh_workflow_status_silent();
                self.record_cockpit_event(
                    "review",
                    "refreshed change review",
                    "/accept <note> or /rework <task>",
                );
                self.status = "Change review refreshed.".into();
            }
            Err(err) => {
                self.status = format!("Could not refresh change review: {err}");
            }
        }
    }

    fn accept_change_review(&mut self, note: &str) {
        self.active_pane = WorkbenchPane::Terminal;
        match record_review_decision(&self.profile.root, "accepted", note) {
            Ok(record) => {
                self.change_review = load_change_review(&self.profile.root)
                    .unwrap_or_else(|err| format!("Could not refresh change review: {err}"));
                self.refresh_workflow_status_silent();
                self.terminal_log = vec![
                    format!("decision: {}", record.decision),
                    format!("note: {}", record.note),
                ];
                self.record_cockpit_event(
                    "review",
                    &format!("accepted: {}", record.note),
                    "commit/push or /new",
                );
                self.status = "Review accepted.".into();
            }
            Err(err) => {
                self.status = format!("Could not record review decision: {err}");
            }
        }
    }

    fn queue_rework_task(&mut self, task: &str) {
        let task = task.trim();
        if task.is_empty() {
            self.status = "Usage: /rework <follow-up task>".into();
            return;
        }
        let text = format!("Review follow-up: {task}");
        match queue_task(&self.profile.root, &text) {
            Ok(record) => {
                self.active_pane = WorkbenchPane::Session;
                self.task_queue.push(record.clone());
                let _ = record_review_decision(&self.profile.root, "rework", task);
                self.refresh_workflow_status_silent();
                self.record_cockpit_event(
                    "rework",
                    &format!("queued follow-up: {task}"),
                    "/run or /route-run",
                );
                self.status = format!("Rework queued: {}", record.id);
            }
            Err(err) => {
                self.status = format!("Could not queue rework task: {err}");
            }
        }
    }

    pub fn start_latest_task_background(&mut self) -> Result<()> {
        self.start_latest_task_background_with(|root, task| run_task_through_harness(&root, &task))
    }

    fn stop_background_run(&mut self) {
        let Ok(Some(run)) = load_background_run(&self.profile.root) else {
            self.active_pane = WorkbenchPane::Terminal;
            self.status = "No active background run to stop.".into();
            self.terminal_log
                .push("No active background run to stop.".into());
            return;
        };
        if !matches!(run.status.as_str(), "running" | "canceling") {
            self.active_pane = WorkbenchPane::Terminal;
            self.status = format!("No active background run to stop: {}", run.status);
            self.terminal_log
                .push(format!("No active background run to stop: {}", run.status));
            return;
        }
        let detail = format!("stop requested for task {}", run.task_id);
        match request_background_cancel(&self.profile.root, &run.task_id, "user requested stop") {
            Ok(_) => {
                if let Ok(state) = record_background_run(
                    &self.profile.root,
                    &run.task_id,
                    "canceling",
                    &detail,
                    run.trace.clone(),
                ) {
                    self.background_run_seen = Some(background_run_marker(&state));
                }
                let _ = append_background_output(&self.profile.root, "system", &detail);
                self.active_pane = WorkbenchPane::Terminal;
                self.terminal_log.push(detail.clone());
                self.record_cockpit_event("background", &detail, "wait for canceled status");
                self.status = format!("Stop requested: {}", run.task_id);
            }
            Err(err) => {
                self.active_pane = WorkbenchPane::Terminal;
                self.status = format!("Could not stop background run: {err}");
                self.terminal_log.push(format!("error: {err}"));
            }
        }
    }

    pub fn start_latest_task_background_with<F>(&mut self, runner: F) -> Result<()>
    where
        F: FnOnce(PathBuf, TaskRecord) -> Result<HarnessRunOutput> + Send + 'static,
    {
        let Some(task) = latest_resumable_task(&self.profile.root)? else {
            self.active_pane = WorkbenchPane::Terminal;
            self.status = "No resumable task found.".into();
            self.terminal_log.push("No resumable task found.".into());
            return Ok(());
        };
        let checkpoint = create_checkpoint(
            &self.profile.root,
            &format!("before task {} background run", task.id),
        )?;
        clear_background_output(&self.profile.root)?;
        clear_background_cancel(&self.profile.root)?;
        self.background_output_seen = 0;
        let trace = PathBuf::from(".argus/tasks").join(format!("{}.trace.jsonl", task.id));
        self.latest_trace_path = Some(trace.clone());
        self.trace_preview =
            load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
        let state = record_background_run(
            &self.profile.root,
            &task.id,
            "running",
            &format!("task {} running in background", task.id),
            Some(trace.clone()),
        )?;
        self.background_run_seen = Some(background_run_marker(&state));

        self.active_pane = WorkbenchPane::Terminal;
        self.terminal_log = vec![
            format!("checkpoint: {}", checkpoint.id),
            format!("background: {}", task.id),
            format!("trace: {}", trace.display()),
        ];
        self.record_cockpit_event(
            "background",
            &format!("started task {} in background", task.id),
            "watch cockpit and trace panels",
        );
        self.status = format!("Background run started: {}", task.id);

        let root = self.profile.root.clone();
        let task_id = task.id.clone();
        let task_text = task.text.clone();
        std::thread::spawn(move || {
            let result = runner(root.clone(), task);
            match result {
                Ok(output) => {
                    let status = output.status.clone();
                    let detail = format!("task {} completed with status {status}", output.task_id);
                    let _ = record_background_run(
                        &root,
                        &output.task_id,
                        &status,
                        &detail,
                        Some(output.trace),
                    );
                }
                Err(err) => {
                    let failure = err.to_string();
                    let _ = update_task_status(&root, &task_id, "failed");
                    let repair_text = build_harness_repair_task(&task_text, &failure);
                    let detail = match queue_task(&root, &repair_text) {
                        Ok(record) => {
                            let repair_line =
                                format!("repair queued: {} {}", record.id, record.text);
                            let _ = append_background_output(&root, "system", &repair_line);
                            format!(
                                "task {task_id} failed: {failure}; repair queued {}",
                                record.id
                            )
                        }
                        Err(queue_err) => {
                            format!("task {task_id} failed: {failure}; repair queue failed: {queue_err}")
                        }
                    };
                    let _ = record_background_run(&root, &task_id, "failed", &detail, None);
                }
            }
        });
        Ok(())
    }

    pub fn run_latest_task_with<F>(&mut self, runner: &mut F) -> Result<()>
    where
        F: FnMut(&Path, &TaskRecord) -> Result<HarnessRunOutput>,
    {
        let Some(task) = latest_resumable_task(&self.profile.root)? else {
            self.active_pane = WorkbenchPane::Terminal;
            self.status = "No resumable task found.".into();
            self.terminal_log.push("No resumable task found.".into());
            return Ok(());
        };
        let checkpoint = create_checkpoint(
            &self.profile.root,
            &format!("before task {} direct run", task.id),
        )?;

        self.active_pane = WorkbenchPane::Terminal;
        self.status = format!("Running task {} through Argus harness...", task.id);
        self.terminal_log = vec![
            format!("checkpoint: {}", checkpoint.id),
            format!("$ arguscode resume --run  # {}", task.text),
        ];
        self.record_cockpit_event(
            "run",
            &format!("starting task {} through harness", task.id),
            "wait for trace and verification result",
        );

        match runner(&self.profile.root, &task) {
            Ok(output) => {
                self.latest_trace_path = Some(output.trace.clone());
                if !output.stdout.trim().is_empty() {
                    self.terminal_log.push(output.stdout);
                }
                if !output.stderr.trim().is_empty() {
                    self.terminal_log.push(output.stderr);
                }
                self.terminal_log.push(format!("status: {}", output.status));
                self.terminal_log
                    .push(format!("trace: {}", output.trace.display()));
                self.task_queue = list_tasks(&self.profile.root)?;
                self.session_history = list_sessions(&self.profile.root)?;
                self.latest_trace_path = self
                    .session_history
                    .last()
                    .map(|session| session.trace.clone())
                    .or_else(|| Some(output.trace.clone()));
                self.diff_preview = load_diff_preview(&self.profile.root)?;
                self.change_review = load_change_review(&self.profile.root)?;
                self.workflow_status =
                    load_workflow_status(&self.profile.root, &self.config.verify.commands)?;
                self.trace_preview =
                    load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
                self.record_cockpit_event(
                    "run",
                    &format!("task {} {}", output.task_id, output.status),
                    "/review, /verify, or /accept <note>",
                );
                self.status = format!("Task {} {}", output.task_id, output.status);
                Ok(())
            }
            Err(err) => {
                let _ = update_task_status(&self.profile.root, &task.id, "failed");
                let repair_task = build_harness_repair_task(&task.text, &err.to_string());
                let repair = queue_task(&self.profile.root, &repair_task);
                self.task_queue = list_tasks(&self.profile.root)?;
                self.session_history = list_sessions(&self.profile.root)?;
                self.latest_trace_path = self
                    .session_history
                    .last()
                    .map(|session| session.trace.clone());
                self.diff_preview = load_diff_preview(&self.profile.root)?;
                self.change_review = load_change_review(&self.profile.root)?;
                self.workflow_status =
                    load_workflow_status(&self.profile.root, &self.config.verify.commands)?;
                self.trace_preview =
                    load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
                self.terminal_log.push(format!("error: {err}"));
                match repair {
                    Ok(record) => {
                        self.terminal_log
                            .push(format!("repair: {} {}", record.id, record.text));
                        self.status =
                            format!("Harness run failed. Repair task queued: {}", record.id);
                        self.record_cockpit_event(
                            "run",
                            &format!("task {} failed; repair queued {}", task.id, record.id),
                            "/run the repair task, /rollback, or /tasks",
                        );
                    }
                    Err(queue_err) => {
                        self.status = format!("Harness run failed: {err}");
                        self.terminal_log
                            .push(format!("could not queue repair task: {queue_err}"));
                        self.record_cockpit_event(
                            "run",
                            &format!("task {} failed: {err}", task.id),
                            "/retry <task-id>, /rework <task>, or /rollback",
                        );
                    }
                }
                Err(err)
            }
        }
    }

    pub fn run_eval_suite_with<F>(&mut self, suite: PathBuf, runner: &mut F) -> Result<()>
    where
        F: FnMut(&Path, &ArgusCodeConfig, &Path) -> Result<EvalRunOutput>,
    {
        self.active_pane = WorkbenchPane::Terminal;
        self.status = format!("Running eval suite {}...", suite.display());
        let checkpoint = create_checkpoint(
            &self.profile.root,
            &format!("before eval {}", suite.display()),
        )?;
        self.terminal_log = vec![
            format!("checkpoint: {}", checkpoint.id),
            format!(
                "$ argus eval {} --provider {} --model {} --in-place",
                suite.display(),
                self.config.provider.default_provider,
                self.config.provider.default_model
            ),
        ];
        self.record_cockpit_event(
            "eval",
            &format!("running suite {}", suite.display()),
            "wait for pass-rate report",
        );

        let output = runner(&self.profile.root, &self.config, &suite)?;
        if !output.stdout.trim().is_empty() {
            self.terminal_log.push(output.stdout);
        }
        if !output.stderr.trim().is_empty() {
            self.terminal_log.push(output.stderr);
        }
        self.terminal_log.push(format!("status: {}", output.status));
        self.terminal_log
            .push(format!("out-dir: {}", output.out_dir.display()));
        self.terminal_log
            .push(format!("report: {}", output.report_json.display()));
        self.eval_dashboard = load_eval_dashboard(&self.profile.root)?;
        self.change_review = load_change_review(&self.profile.root)?;
        self.workflow_status =
            load_workflow_status(&self.profile.root, &self.config.verify.commands)?;
        self.record_cockpit_event(
            "eval",
            &format!("suite {} {}", output.suite.display(), output.status),
            "/review, /rework <task>, or inspect report",
        );
        self.status = if output.status == "passed" {
            format!("Eval passed: {}", output.suite.display())
        } else {
            format!("Eval failed: {}", output.suite.display())
        };
        Ok(())
    }

    pub fn run_latest_task_with_route<F>(
        &mut self,
        cheap_model: &str,
        strong_model: &str,
        runner: &mut F,
    ) -> Result<()>
    where
        F: FnMut(&Path, &TaskRecord, &str, &str) -> Result<RouteRunOutput>,
    {
        let Some(task) = latest_resumable_task(&self.profile.root)? else {
            self.active_pane = WorkbenchPane::Terminal;
            self.status = "No resumable task found.".into();
            self.terminal_log.push("No resumable task found.".into());
            return Ok(());
        };
        let checkpoint = create_checkpoint(
            &self.profile.root,
            &format!("before task {} route run", task.id),
        )?;

        self.active_pane = WorkbenchPane::Terminal;
        self.status = format!(
            "Routing task {} through {} -> {}...",
            task.id, cheap_model, strong_model
        );
        self.terminal_log = vec![
            format!("checkpoint: {}", checkpoint.id),
            format!(
                "$ argus route {} --cheap {} --strong {}",
                task.text, cheap_model, strong_model
            ),
        ];
        self.record_cockpit_event(
            "route",
            &format!(
                "starting task {} through {} -> {}",
                task.id, cheap_model, strong_model
            ),
            "wait for route decision and trace",
        );

        match runner(&self.profile.root, &task, cheap_model, strong_model) {
            Ok(output) => {
                if !output.stdout.trim().is_empty() {
                    self.terminal_log.push(output.stdout);
                }
                if !output.stderr.trim().is_empty() {
                    self.terminal_log.push(output.stderr);
                }
                self.terminal_log.push(format!(
                    "route: {} -> {}",
                    output.cheap_model, output.strong_model
                ));
                self.terminal_log.push(format!("status: {}", output.status));
                self.terminal_log
                    .push(format!("trace: {}", output.trace.display()));
                self.task_queue = list_tasks(&self.profile.root)?;
                self.session_history = list_sessions(&self.profile.root)?;
                self.latest_trace_path = self
                    .session_history
                    .last()
                    .map(|session| session.trace.clone())
                    .or_else(|| Some(output.trace.clone()));
                self.diff_preview = load_diff_preview(&self.profile.root)?;
                self.change_review = load_change_review(&self.profile.root)?;
                self.workflow_status =
                    load_workflow_status(&self.profile.root, &self.config.verify.commands)?;
                self.trace_preview =
                    load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
                self.record_cockpit_event(
                    "route",
                    &format!(
                        "task {} {} via {} -> {}",
                        output.task_id, output.status, output.cheap_model, output.strong_model
                    ),
                    "/review, /verify, or /accept <note>",
                );
                self.status = format!(
                    "Route task {} {}: {} -> {}",
                    output.task_id, output.status, output.cheap_model, output.strong_model
                );
                Ok(())
            }
            Err(err) => {
                self.task_queue = list_tasks(&self.profile.root)?;
                self.session_history = list_sessions(&self.profile.root)?;
                self.latest_trace_path = self
                    .session_history
                    .last()
                    .map(|session| session.trace.clone());
                self.diff_preview = load_diff_preview(&self.profile.root)?;
                self.change_review = load_change_review(&self.profile.root)?;
                self.workflow_status =
                    load_workflow_status(&self.profile.root, &self.config.verify.commands)?;
                self.trace_preview =
                    load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
                self.status = format!("Route run failed: {err}");
                self.terminal_log.push(format!("error: {err}"));
                self.record_cockpit_event(
                    "route",
                    &format!("task {} failed: {err}", task.id),
                    "/retry <task-id>, /rework <task>, or /rollback",
                );
                Err(err)
            }
        }
    }

    pub fn run_verify_gate(&mut self) -> Result<()> {
        self.active_pane = WorkbenchPane::Terminal;
        self.terminal_log = if self.config.verify.commands.is_empty() {
            vec!["No verification command configured.".into()]
        } else {
            self.config
                .verify
                .commands
                .iter()
                .map(|command| format!("$ {command}"))
                .collect()
        };
        self.record_cockpit_event(
            "verify",
            &format!("running {} command(s)", self.config.verify.commands.len()),
            "wait for verification gate",
        );

        let output = run_configured_verify(&self.profile.root, &self.config.verify.commands)?;
        self.terminal_log.push(output.detail.clone());
        if output.passed {
            self.terminal_log.push("verification passed".into());
            self.refresh_workflow_status_silent();
            self.record_cockpit_event("verify", &output.detail, "/review or /accept <note>");
            self.status = format!("Verification passed: {} command(s)", output.commands.len());
            Ok(())
        } else {
            self.terminal_log.push("verification failed".into());
            let repair_text = build_repair_task(&output.commands, &output.detail);
            match queue_task(&self.profile.root, &repair_text) {
                Ok(record) => {
                    self.task_queue.push(record.clone());
                    self.terminal_log
                        .push(format!("repair task: {}", record.id));
                    self.refresh_workflow_status_silent();
                    self.record_cockpit_event(
                        "repair",
                        &format!("queued repair task: {}", record.id),
                        "/run or /route-run",
                    );
                    self.status = format!("Verification failed. Repair task queued: {}", record.id);
                }
                Err(err) => {
                    self.refresh_workflow_status_silent();
                    self.status =
                        format!("Verification failed; repair task could not be queued: {err}");
                }
            }
            self.record_cockpit_event(
                "verify",
                &format!("failed: {}", output.detail),
                "/run repair task or inspect terminal output",
            );
            anyhow::bail!(output.detail)
        }
    }
}

fn background_run_marker(run: &BackgroundRun) -> String {
    format!("{}:{}:{}", run.task_id, run.status, run.updated_ms)
}

fn build_progress_summary(
    status: &str,
    trace: &TracePreview,
    cockpit_journal: &str,
    terminal_log: &[String],
) -> String {
    let mut lines = vec![
        "Progress Summary".to_string(),
        format!("Status: {}", compact_progress_line(status, 96)),
    ];
    if let Some(trace_line) = trace.lines.last() {
        lines.push(format!("Trace: {}", compact_progress_line(trace_line, 112)));
    } else {
        lines.push(format!(
            "Trace: {}",
            compact_progress_line(&trace.headline, 112)
        ));
    }
    if let Some(cockpit_line) = cockpit_journal.lines().rev().find(|line| {
        let line = line.trim();
        !line.is_empty() && line != "Execution Cockpit" && !line.starts_with("next:")
    }) {
        lines.push(format!(
            "Cockpit: {}",
            compact_progress_line(cockpit_line, 112)
        ));
    }
    if let Some(output_line) = terminal_log
        .iter()
        .rev()
        .flat_map(|entry| entry.lines().rev())
        .find(|line| !line.trim().is_empty())
    {
        lines.push(format!(
            "Output: {}",
            compact_progress_line(output_line, 112)
        ));
    }
    lines.join("\n")
}

fn render_progress_summary_compact(app: &WorkbenchApp) -> String {
    let trace = app
        .trace_preview
        .lines
        .last()
        .map(|line| compact_progress_line(line, 64))
        .unwrap_or_else(|| compact_progress_line(&app.trace_preview.headline, 64));
    format!(
        "Progress Summary\nStatus: {} | Trace: {trace}",
        compact_progress_line(&app.status, 72)
    )
}

fn compact_progress_line(value: &str, max_chars: usize) -> String {
    let normalized = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut out = normalized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

pub fn handle_key(app: &mut WorkbenchApp, code: KeyCode, modifiers: KeyModifiers) -> bool {
    match app.mode {
        WorkbenchMode::Help => match code {
            KeyCode::Esc | KeyCode::Char('?') => app.close_overlay(),
            KeyCode::Char('q') if modifiers.is_empty() => return false,
            _ => {}
        },
        WorkbenchMode::CommandPalette => match code {
            KeyCode::Esc => app.close_overlay(),
            KeyCode::Down => app.palette_next(),
            KeyCode::Up => app.palette_prev(),
            KeyCode::Enter => app.execute_palette_action(),
            KeyCode::Backspace => app.pop_palette_query(),
            KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => app.close_overlay(),
            KeyCode::Char(c) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                app.push_palette_query(c);
            }
            _ => {}
        },
        WorkbenchMode::Normal => match code {
            KeyCode::Char('q') if modifiers.is_empty() => return false,
            KeyCode::Esc => return false,
            KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => app.open_palette(),
            KeyCode::Char('?') if modifiers.is_empty() => {
                app.mode = WorkbenchMode::Help;
                app.status = "Help open. Press ? or Esc to close.".into();
            }
            KeyCode::Tab => {
                if !app.complete_slash_command() {
                    app.next_pane();
                }
            }
            KeyCode::Enter => app.submit_input(),
            KeyCode::Backspace => app.pop_input(),
            KeyCode::Char(c) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
                app.push_input(c);
            }
            _ => {}
        },
    }
    true
}

pub fn ensure_config(root: &Path) -> Result<(ProjectProfile, ArgusCodeConfig)> {
    let profile = detect_project(root)?;
    let config_path = ArgusCodeConfig::path(&profile.root);
    if !config_path.exists() {
        init_project(&profile.root, false)?;
    }
    let config = ArgusCodeConfig::read(&profile.root)?;
    Ok((profile, config))
}

pub fn run_workbench(start: &Path) -> Result<()> {
    let (profile, config) = ensure_config(start)?;
    let mut app = WorkbenchApp::load(profile, config)?;

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut WorkbenchApp,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;
        if event::poll(Duration::from_millis(250))? {
            let Event::Key(key) = event::read()? else {
                app.tick();
                continue;
            };
            if key.kind == KeyEventKind::Press {
                match key.code {
                    _ if !handle_key(app, key.code, key.modifiers) => break,
                    _ => {}
                }
            }
        }
        app.tick();
    }
    Ok(())
}

pub fn ui(f: &mut Frame, app: &WorkbenchApp) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(10),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, outer[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Min(42),
            Constraint::Length(34),
        ])
        .split(outer[1]);

    render_project(f, app, body[0]);
    render_session(f, app, body[1]);
    render_trace(f, app, body[2]);
    render_terminal(f, app, outer[2]);
    render_status(f, app, outer[3]);

    match app.mode {
        WorkbenchMode::CommandPalette => render_command_palette(f, app),
        WorkbenchMode::Help => render_help(f),
        WorkbenchMode::Normal => {}
    }
}

fn render_header(f: &mut Frame, app: &WorkbenchApp, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            " ArgusCode ",
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(" repo: "),
        Span::styled(
            &app.profile.name,
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | model: "),
        Span::styled(
            format!(
                "{}/{}",
                app.config.provider.default_provider, app.config.provider.default_model
            ),
            Style::default().fg(Color::Green),
        ),
        Span::raw(" | gate: "),
        Span::styled(
            if app.config.verify.gate { "on" } else { "off" },
            Style::default().fg(if app.config.verify.gate {
                Color::Green
            } else {
                Color::Yellow
            }),
        ),
        Span::raw(" | sandbox: "),
        Span::styled(
            &app.config.security.sandbox,
            Style::default().fg(Color::Blue),
        ),
        Span::raw(" | approval: "),
        Span::styled(
            &app.config.security.approval,
            Style::default().fg(Color::Magenta),
        ),
        Span::raw(" | harness: live"),
    ]);
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_project(f: &mut Frame, app: &WorkbenchApp, area: Rect) {
    let mut items = vec![
        ListItem::new(Line::from(vec![
            Span::styled("root ", Style::default().fg(Color::DarkGray)),
            Span::raw(app.profile.root.display().to_string()),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled("lang ", Style::default().fg(Color::DarkGray)),
            Span::raw(if app.profile.languages.is_empty() {
                "unknown".into()
            } else {
                app.profile.languages.join(", ")
            }),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled("pkg  ", Style::default().fg(Color::DarkGray)),
            Span::raw(
                app.profile
                    .package_manager
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_string(),
            ),
        ])),
        ListItem::new(""),
        ListItem::new("Queue"),
        ListItem::new(format!("{} queued", app.task_queue.len())),
    ];
    for task in app.task_queue.iter().rev().take(3) {
        items.push(ListItem::new(format!("[{}] {}", task.status, task.text)));
    }
    if !app.profile.rules_files.is_empty() {
        items.push(ListItem::new(""));
        items.push(ListItem::new("Imported rules"));
        for path in &app.profile.rules_files {
            items.push(ListItem::new(format!("• {}", path.display())));
        }
    }
    items.push(ListItem::new(""));
    for line in app.repo_map.lines().take(16) {
        items.push(ListItem::new(line.to_string()));
    }
    f.render_widget(
        List::new(items).block(panel_block(
            "Project",
            app.active_pane == WorkbenchPane::Project,
        )),
        area,
    );
}

fn render_session(f: &mut Frame, app: &WorkbenchApp, area: Rect) {
    let verify = if app.config.verify.commands.is_empty() {
        "No verify command detected".to_string()
    } else {
        app.config.verify.commands.join("\n")
    };
    let queue = if app.task_queue.is_empty() {
        "(empty)".to_string()
    } else {
        app.task_queue
            .iter()
            .rev()
            .take(3)
            .map(|task| format!("[{}] {}", task.status, task.text))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let review_loop = app
        .change_review
        .lines()
        .take(7)
        .collect::<Vec<_>>()
        .join("\n");
    let workflow_status = app
        .workflow_status
        .lines()
        .take(7)
        .collect::<Vec<_>>()
        .join("\n");
    let plan_status = app
        .plan_status
        .lines()
        .take(2)
        .collect::<Vec<_>>()
        .join("\n");
    let diff_preview = app
        .diff_preview
        .lines()
        .take(10)
        .collect::<Vec<_>>()
        .join("\n");
    let command_suggestions = render_slash_command_suggestions(&app.input);
    let command_suggestions = if command_suggestions.is_empty() {
        String::new()
    } else {
        format!("\n\n{command_suggestions}")
    };
    let text = format!(
        "Chat\n> {}{}\n\n{}\n\nTask Queue\n{}\n\nDiff Preview\n{}\n\nPlan\n{}\n\nReview Loop\n{}\n\nVerify Profile\n{}",
        if app.input.is_empty() {
            "Type a task here, then press Enter.".to_string()
        } else {
            app.input.clone()
        },
        command_suggestions,
        workflow_status,
        queue,
        diff_preview,
        plan_status,
        review_loop,
        verify
    );
    f.render_widget(
        Paragraph::new(text)
            .block(panel_block(
                "Conversation / Flow / Diff",
                app.active_pane == WorkbenchPane::Session,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_trace(f: &mut Frame, app: &WorkbenchApp, area: Rect) {
    let mut lines = vec![
        Line::from("Trace Timeline"),
        Line::from(app.trace_preview.headline.clone()),
        Line::from(""),
        Line::from("Trace target"),
        Line::from(app.trace_preview.target.clone()),
    ];
    if app.trace_preview.lines.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("(no trace events loaded)"));
    } else {
        lines.push(Line::from(""));
        for line in &app.trace_preview.lines {
            lines.push(Line::from(line.clone()));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Session History"));
    if app.session_history.is_empty() {
        lines.push(Line::from("(empty)"));
    } else {
        for session in app.session_history.iter().rev().take(3) {
            lines.push(Line::from(format!(
                "[{}] {}",
                session.status, session.task_text
            )));
            lines.push(Line::from(session.trace.display().to_string()));
        }
    }
    lines.push(Line::from(""));
    for line in app.eval_dashboard.lines().take(6) {
        lines.push(Line::from(line.to_string()));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Memory"));
    for line in app.memory_preview.lines().take(4) {
        lines.push(Line::from(line.to_string()));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(panel_block(
                "Trace Timeline / Memory",
                app.active_pane == WorkbenchPane::Trace,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_terminal(f: &mut Frame, app: &WorkbenchApp, area: Rect) {
    let output = if !app.terminal_log.is_empty() {
        app.terminal_log.join("\n")
    } else if app.config.verify.commands.is_empty() {
        "No verification command configured.".to_string()
    } else {
        app.config
            .verify
            .commands
            .iter()
            .map(|cmd| format!("$ {cmd}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let cockpit = app
        .cockpit_journal
        .lines()
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    let progress = render_progress_summary_compact(app);
    let commands = format!("{progress}\n\nOutput\n{output}\n\n{cockpit}");
    f.render_widget(
        Paragraph::new(commands)
            .block(panel_block(
                "Execution Cockpit / Terminal",
                app.active_pane == WorkbenchPane::Terminal,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_status(f: &mut Frame, app: &WorkbenchApp, area: Rect) {
    let tab_hint = if !slash_command_suggestions(&app.input).is_empty() {
        "Tab complete"
    } else {
        "Tab pane"
    };
    let status = format!(
        " {} · {} · Enter queue · Esc/q quit · config {} ",
        app.status, tab_hint, CONFIG_PATH
    );
    f.render_widget(Paragraph::new(status), area);
}

fn slash_command_prefix(input: &str) -> Option<&str> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    Some(trimmed)
}

fn slash_command_suggestions(input: &str) -> Vec<&'static SlashCommandHint> {
    let Some(prefix) = slash_command_prefix(input) else {
        return Vec::new();
    };
    SLASH_COMMAND_HINTS
        .iter()
        .filter(|hint| hint.command.starts_with(prefix))
        .take(6)
        .collect()
}

fn render_slash_command_suggestions(input: &str) -> String {
    let suggestions = slash_command_suggestions(input);
    if suggestions.is_empty() {
        return String::new();
    }
    let mut lines = vec!["Command Suggestions".to_string()];
    lines.extend(
        suggestions
            .iter()
            .map(|hint| format!("{}  -  {}", hint.command, hint.detail)),
    );
    lines.join("\n")
}

fn unknown_slash_command_status(command: &str) -> String {
    if let Some(nearest) = nearest_slash_command(command) {
        format!(
            "Unknown slash command: {command}. Did you mean {}? Try /commands or /help.",
            nearest.command
        )
    } else {
        format!(
            "Unknown slash command: {command}. Try /commands, /help, /verify, /run, /eval-run, /flow."
        )
    }
}

fn nearest_slash_command(command: &str) -> Option<&'static SlashCommandHint> {
    SLASH_COMMAND_HINTS
        .iter()
        .map(|hint| (edit_distance(command, hint.command), hint))
        .min_by_key(|(distance, _)| *distance)
        .and_then(|(distance, hint)| (distance <= 3).then_some(hint))
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b_chars = b.chars().collect::<Vec<_>>();
    let mut costs = (0..=b_chars.len()).collect::<Vec<_>>();

    for (i, a_char) in a.chars().enumerate() {
        let mut previous_diagonal = costs[0];
        costs[0] = i + 1;
        for (j, b_char) in b_chars.iter().enumerate() {
            let previous_cost = costs[j + 1];
            let substitution = previous_diagonal + usize::from(a_char != *b_char);
            let insertion = costs[j] + 1;
            let deletion = previous_cost + 1;
            costs[j + 1] = substitution.min(insertion).min(deletion);
            previous_diagonal = previous_cost;
        }
    }

    costs[b_chars.len()]
}

fn render_command_palette(f: &mut Frame, app: &WorkbenchApp) {
    let area = centered_rect(66, 52, f.area());
    f.render_widget(Clear, area);
    let filtered = filtered_palette_items(&app.palette_query);
    let mut items = vec![ListItem::new(format!(
        "Search: {}",
        if app.palette_query.is_empty() {
            "(type to filter)"
        } else {
            app.palette_query.as_str()
        }
    ))];
    if filtered.is_empty() {
        items.push(ListItem::new(format!(
            "No command matches '{}'",
            app.palette_query
        )));
    } else {
        items.extend(
            filtered
                .iter()
                .map(|item| ListItem::new(format!("{}  -  {}", item.label, item.detail))),
        );
    }
    let selected = if filtered.is_empty() {
        None
    } else {
        Some(app.palette_selected + 1)
    };
    let mut state = ListState::default();
    state.select(selected);
    let title = if app.palette_query.is_empty() {
        "Command Palette".to_string()
    } else {
        format!("Command Palette - {} match(es)", filtered.len())
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    f.render_stateful_widget(list, area, &mut state);
}

fn filtered_palette_items(query: &str) -> Vec<&'static PaletteItem> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return PALETTE_ITEMS.iter().collect();
    }
    PALETTE_ITEMS
        .iter()
        .filter(|item| item.matches_query(&query))
        .collect()
}

fn render_help(f: &mut Frame) {
    let area = centered_rect(62, 48, f.area());
    f.render_widget(Clear, area);
    let text = "ArgusCode Help\n\n\
Ctrl+K  Open searchable command palette\n\
Tab     Switch pane / complete slash command\n\
Enter   Queue task / run selected command\n\
?       Toggle help\n\
Esc     Close overlay or exit\n\
q       Quit\n\n\
Harness flow\n\
plan -> edit -> verify -> repair -> trace\n\n\
Slash commands\n\
/verify  Run verification gate\n\
/check   Run verification gate\n\
/ask     Queue a task from a familiar agent-style prompt\n\
/add     Queue a task\n\
/code    Queue a coding task\n\
/edit    Queue a coding task\n\
/fix     Queue a fix task\n\
/implement Queue an implementation task\n\
/run     Run latest queued task\n\
/continue Run latest queued task\n\
/stop    Stop active background run\n\
/logs    Open terminal output\n\
/trace   Open trace timeline\n\
/commands Search familiar agent command mappings\n\
/cheatsheet Search familiar agent command mappings\n\
/migrate Search familiar agent command mappings\n\
/route-run Route latest task through cheap/strong models\n\
/map     Refresh repo map\n\
/evals   Refresh eval dashboard\n\
/eval-run Run smoke eval or a suite path\n\
/doctor  Show agent compatibility\n\
/health  Show agent compatibility\n\
/compat  Show agent compatibility\n\
/tasks   Show task queue\n\
/cancel  Cancel a task by id\n\
/retry   Requeue a task by id\n\
/flow    Refresh workflow status\n\
/status  Refresh workflow status\n\
/plan    Create a durable work plan\n\
/next    Queue the next plan step\n\
/done    Complete the current plan step with evidence\n\
/launch  Show launch readiness checklist\n\
/diff    Refresh diff preview\n\
/review  Refresh change review\n\
/patch   Refresh patch review\n\
/accept  Record accepted review decision\n\
/rework  Queue a review follow-up task\n\
/history Open session history\n\
/memory  Refresh project memory preview\n\
/remember Append a durable lesson\n\
/mcp     Set/show MCP server command\n\
/mcp-allow Allow an MCP tool by name\n\
/checkpoint Save a rollback snapshot\n\
/rollback Restore latest or named checkpoint\n\
/model   Set or show current model\n\
/provider Set or show provider profile\n\
/sandbox Set or show sandbox profile\n\
/approval Set or show approval profile";
    f.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title("ArgusCode Help"),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn panel_block(title: &'static str, active: bool) -> Block<'static> {
    let style = if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Gray)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval_runner::EvalRunOutput;
    use crate::harness::HarnessRunOutput;
    use crate::project::build_config;
    use crate::route_runner::RouteRunOutput;
    use crate::sessions::append_session;
    use crate::tasks::{list_tasks, queue_task};
    use argus_trace::{EventKind, TraceWriter};
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;

    fn app() -> WorkbenchApp {
        app_with_root(PathBuf::from("/tmp/demo"))
    }

    fn app_with_root(root: PathBuf) -> WorkbenchApp {
        let profile = ProjectProfile {
            root,
            name: "demo".into(),
            languages: vec!["rust".into()],
            package_manager: Some("cargo".into()),
            verify_commands: vec!["cargo test --workspace --locked".into()],
            rules_files: vec![PathBuf::from("AGENTS.md")],
            detected_files: vec![PathBuf::from("Cargo.toml")],
        };
        let config = build_config(&profile);
        WorkbenchApp::new(profile, config)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "arguscode-workbench-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn tab_cycles_panes() {
        let mut app = app();
        assert_eq!(app.active_pane, WorkbenchPane::Session);
        app.next_pane();
        assert_eq!(app.active_pane, WorkbenchPane::Trace);
        app.next_pane();
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        app.next_pane();
        assert_eq!(app.active_pane, WorkbenchPane::Project);
    }

    #[test]
    fn ui_renders_workbench_regions() {
        let app = app();
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("ArgusCode"), "{text}");
        assert!(text.contains("Project"), "{text}");
        assert!(text.contains("Conversation"), "{text}");
        assert!(text.contains("Trace"), "{text}");
        assert!(text.contains("Terminal"), "{text}");
        assert!(text.contains("cargo test"), "{text}");
    }

    #[test]
    fn workbench_loads_and_renders_workflow_status() {
        let dir = temp_dir("load-workflow");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        let seed = app_with_root(dir.clone());
        queue_task(&dir, "tighten workflow status").unwrap();

        let app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        assert!(
            app.workflow_status.contains("Phase: Task queued"),
            "{}",
            app.workflow_status
        );
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Workflow Status"), "{text}");
        assert!(text.contains("tighten workflow status"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workbench_loads_and_renders_execution_cockpit() {
        let dir = temp_dir("load-cockpit");
        std::fs::create_dir_all(&dir).unwrap();
        let seed = app_with_root(dir.clone());
        crate::cockpit::append_cockpit_event(&dir, "run", "task task-1 done", "/review").unwrap();

        let app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        assert!(
            app.cockpit_journal.contains("Execution Cockpit"),
            "{}",
            app.cockpit_journal
        );
        assert!(
            app.cockpit_journal.contains("task task-1 done"),
            "{}",
            app.cockpit_journal
        );
        let mut terminal = Terminal::new(TestBackend::new(120, 34)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Execution Cockpit"), "{text}");
        assert!(text.contains("task task-1 done"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn progress_summary_combines_trace_cockpit_and_output() {
        let mut app = app();
        app.status = "Background run active: task-1".into();
        app.trace_preview = TracePreview {
            target: ".argus/tasks/task-1.trace.jsonl".into(),
            headline: "3 events".into(),
            lines: vec![
                "[  0] TASK     implement live progress".into(),
                "[  1] MODEL <- deepseek-reasoner (10+20 tokens): editing".into(),
                "[  2] TOOL ->  shell({\"cmd\":\"cargo test\"})".into(),
            ],
        };
        app.cockpit_journal =
            "Execution Cockpit\n[harness] task task-1 running\nnext: wait for verification".into();
        app.terminal_log = vec![
            "[stdout] compiling argus".into(),
            "[stderr] warning: retrying".into(),
        ];

        app.refresh_progress_summary_silent();

        assert!(
            app.progress_summary.contains("Progress Summary"),
            "{}",
            app.progress_summary
        );
        assert!(
            app.progress_summary.contains("Background run active"),
            "{}",
            app.progress_summary
        );
        assert!(
            app.progress_summary.contains("Trace: [  2] TOOL"),
            "{}",
            app.progress_summary
        );
        assert!(
            app.progress_summary
                .contains("Cockpit: [harness] task task-1 running"),
            "{}",
            app.progress_summary
        );
        assert!(
            app.progress_summary
                .contains("Output: [stderr] warning: retrying"),
            "{}",
            app.progress_summary
        );
    }

    #[test]
    fn terminal_renders_progress_summary_before_output() {
        let mut app = app();
        app.status = "running".into();
        app.trace_preview = TracePreview {
            target: ".argus/tasks/task-1.trace.jsonl".into(),
            headline: "2 events".into(),
            lines: vec!["[  1] TOOL ->  shell({\"cmd\":\"cargo test\"})".into()],
        };
        app.terminal_log = vec!["[stdout] compiling".into()];

        let mut terminal = Terminal::new(TestBackend::new(120, 34)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(text.contains("Progress Summary"), "{text}");
        assert!(text.contains("Trace: [  1] TOOL"), "{text}");
        assert!(text.contains("Output"), "{text}");
        assert!(text.contains("[stdout] compiling"), "{text}");
    }

    #[test]
    fn command_palette_runs_verify_gate() {
        let dir = temp_dir("verify-action");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("marker.txt"), "ok\n").unwrap();
        let mut app = app_with_root(dir.clone());
        app.config.verify.commands = vec!["test -f marker.txt".into()];

        assert_eq!(app.mode, WorkbenchMode::Normal);
        assert!(handle_key(
            &mut app,
            KeyCode::Char('k'),
            KeyModifiers::CONTROL
        ));
        assert_eq!(app.mode, WorkbenchMode::CommandPalette);
        assert!(app.status.contains("Command palette"), "{}", app.status);

        assert!(handle_key(&mut app, KeyCode::Down, KeyModifiers::empty()));
        assert!(handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty()));
        assert_eq!(app.mode, WorkbenchMode::Normal);
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(app.status.contains("Verification passed"), "{}", app.status);
        assert!(
            app.terminal_log.join("\n").contains("$ test -f marker.txt"),
            "{:?}",
            app.terminal_log
        );
        assert!(
            app.terminal_log.join("\n").contains("1 check(s) passed"),
            "{:?}",
            app.terminal_log
        );

        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("test -f marker.txt"), "{text}");
        assert!(text.contains("verification passed"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_palette_navigation_executes_memory_action() {
        let mut app = app();

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Down, KeyModifiers::empty());
        handle_key(&mut app, KeyCode::Down, KeyModifiers::empty());
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.active_pane, WorkbenchPane::Trace);
        assert!(app.status.contains("Memory"), "{}", app.status);
    }

    #[test]
    fn command_palette_opens_session_history() {
        let dir = temp_dir("history-action");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());
        append_session(
            &dir,
            "task-1",
            "review the diff",
            "done",
            ".argus/tasks/task-1.trace.jsonl",
        )
        .unwrap();
        app = WorkbenchApp::load(app.profile, app.config).unwrap();

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Down, KeyModifiers::empty());
        handle_key(&mut app, KeyCode::Down, KeyModifiers::empty());
        handle_key(&mut app, KeyCode::Down, KeyModifiers::empty());
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.active_pane, WorkbenchPane::Trace);
        assert!(app.status.contains("Session history"), "{}", app.status);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_palette_refreshes_diff_preview() {
        let dir = temp_dir("refresh-diff");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        let mut app = app_with_root(dir.clone());
        app.diff_preview = "stale".into();
        std::fs::write(dir.join("later.txt"), "later\n").unwrap();

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        for _ in 0..4 {
            handle_key(&mut app, KeyCode::Down, KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.active_pane, WorkbenchPane::Session);
        assert!(
            app.status.contains("Diff preview refreshed"),
            "{}",
            app.status
        );
        assert!(
            app.diff_preview.contains("later.txt"),
            "{}",
            app.diff_preview
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_palette_filters_items_by_typed_query() {
        let mut app = app();

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        for c in "diff".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }

        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(text.contains("Search: diff"), "{text}");
        assert!(text.contains("Refresh diff preview"), "{text}");
        assert!(!text.contains("Run latest queued task"), "{text}");
    }

    #[test]
    fn command_palette_executes_filtered_selection() {
        let dir = temp_dir("palette-search-diff");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        let mut app = app_with_root(dir.clone());
        app.diff_preview = "stale".into();
        std::fs::write(dir.join("filtered.txt"), "filtered\n").unwrap();

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        for c in "diff".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.mode, WorkbenchMode::Normal);
        assert!(
            app.status.contains("Diff preview refreshed"),
            "{}",
            app.status
        );
        assert!(
            app.diff_preview.contains("filtered.txt"),
            "{}",
            app.diff_preview
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_palette_keeps_open_when_query_has_no_matches() {
        let mut app = app();

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        for c in "zzzz".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.mode, WorkbenchMode::CommandPalette);
        assert!(app.status.contains("No command matches"), "{}", app.status);
    }

    #[test]
    fn command_palette_opens_agent_command_guide() {
        let mut app = app();
        let index = PALETTE_ITEMS
            .iter()
            .position(|item| item.label.contains("agent command guide"))
            .expect("agent command guide palette item");

        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
        app.palette_selected = index;
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        let terminal = app.terminal_log.join("\n");
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(terminal.contains("Agent command guide"), "{terminal}");
        assert!(terminal.contains("arguscode fix"), "{terminal}");
        assert!(app.status.contains("command guide"), "{}", app.status);
    }

    #[test]
    fn help_overlay_toggles_from_keyboard_and_renders_shortcuts() {
        let mut app = app();

        assert!(handle_key(
            &mut app,
            KeyCode::Char('?'),
            KeyModifiers::empty()
        ));
        assert_eq!(app.mode, WorkbenchMode::Help);

        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("ArgusCode Help"), "{text}");
        assert!(text.contains("Ctrl+K"), "{text}");
    }

    #[test]
    fn slash_commands_filter_agent_command_guide() {
        let mut app = app();

        for c in "/commands fix".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        let terminal = app.terminal_log.join("\n");
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(terminal.contains("Agent command guide"), "{terminal}");
        assert!(terminal.contains("Filter: fix"), "{terminal}");
        assert!(terminal.contains("arguscode fix"), "{terminal}");
        assert!(terminal.contains("/fix"), "{terminal}");
        assert!(!terminal.contains("arguscode provider"), "{terminal}");
        assert!(app.status.contains("command guide"), "{}", app.status);
    }

    #[test]
    fn session_renders_slash_command_suggestions_while_typing() {
        let mut app = app();
        app.input = "/co".into();

        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(text.contains("Command Suggestions"), "{text}");
        assert!(text.contains("/commands"), "{text}");
        assert!(text.contains("/continue"), "{text}");
        assert!(text.contains("Tab complete"), "{text}");
    }

    #[test]
    fn unknown_slash_command_suggests_nearest_known_command() {
        let mut app = app();

        for c in "/verfy".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(
            app.status.contains("Unknown slash command: /verfy"),
            "{}",
            app.status
        );
        assert!(
            app.status.contains("Did you mean /verify?"),
            "{}",
            app.status
        );
    }

    #[test]
    fn tab_completes_first_slash_command_suggestion() {
        let mut app = app();
        app.input = "/ver".into();
        let active_pane = app.active_pane;

        handle_key(&mut app, KeyCode::Tab, KeyModifiers::empty());

        assert_eq!(app.input, "/verify ");
        assert_eq!(app.active_pane, active_pane);
        assert!(
            app.status.contains("Completed slash command"),
            "{}",
            app.status
        );
    }

    #[test]
    fn workbench_loads_existing_task_queue() {
        let dir = temp_dir("load-queue");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());
        queue_task(&dir, "ship the queue panel").unwrap();

        app = WorkbenchApp::load(app.profile, app.config).unwrap();

        assert_eq!(app.task_queue.len(), 1);
        assert_eq!(app.task_queue[0].text, "ship the queue panel");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enter_queues_task_to_disk_and_renders_queue() {
        let dir = temp_dir("submit-queue");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "fix flaky parser tests".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.input.is_empty());
        assert_eq!(app.task_queue.len(), 1);
        assert_eq!(list_tasks(&dir).unwrap()[0].text, "fix flaky parser tests");
        assert!(app.status.contains("Queued task"), "{}", app.status);
        assert!(
            app.cockpit_journal.contains("queued task"),
            "{}",
            app.cockpit_journal
        );
        assert!(
            app.workflow_status.contains("Phase: Task queued"),
            "{}",
            app.workflow_status
        );

        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("fix flaky parser tests"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_flow_refreshes_workflow_status_without_queueing_task() {
        let dir = temp_dir("slash-flow");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        let mut app = app_with_root(dir.clone());
        app.workflow_status = "stale".into();
        std::fs::write(dir.join("flow.txt"), "flow\n").unwrap();

        for c in "/flow".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Session);
        assert!(
            app.status.contains("Workflow status refreshed"),
            "{}",
            app.status
        );
        assert!(
            app.workflow_status.contains("Phase: Review needed"),
            "{}",
            app.workflow_status
        );
        assert!(
            app.workflow_status.contains("Workspace: 1 changed path(s)"),
            "{}",
            app.workflow_status
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_verify_runs_gate_without_queueing_task() {
        let dir = temp_dir("slash-verify");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("marker.txt"), "ok\n").unwrap();
        let mut app = app_with_root(dir.clone());
        app.config.verify.commands = vec!["test -f marker.txt".into()];

        for c in "/verify".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(app.status.contains("Verification passed"), "{}", app.status);
        assert!(
            app.terminal_log.join("\n").contains("$ test -f marker.txt"),
            "{:?}",
            app.terminal_log
        );
        assert!(
            app.cockpit_journal.contains("Execution Cockpit"),
            "{}",
            app.cockpit_journal
        );
        assert!(
            app.cockpit_journal.contains("1 check(s) passed"),
            "{}",
            app.cockpit_journal
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_verify_failure_queues_repair_task() {
        let dir = temp_dir("slash-verify-repair");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());
        app.config.verify.commands = vec!["test -f missing.txt".into()];

        for c in "/verify".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.task_queue.len(), 1, "{:?}", app.task_queue);
        assert!(
            app.task_queue[0]
                .text
                .contains("Repair verification failure"),
            "{:?}",
            app.task_queue
        );
        assert!(
            app.task_queue[0].text.contains("test -f missing.txt"),
            "{:?}",
            app.task_queue
        );
        assert!(app.status.contains("Repair task queued"), "{}", app.status);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_check_runs_verify_alias_without_queueing_task() {
        let dir = temp_dir("slash-check");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("marker.txt"), "ok\n").unwrap();
        let mut app = app_with_root(dir.clone());
        app.config.verify.commands = vec!["test -f marker.txt".into()];

        for c in "/check".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(app.status.contains("Verification passed"), "{}", app.status);
        assert!(
            app.terminal_log.join("\n").contains("$ test -f marker.txt"),
            "{:?}",
            app.terminal_log
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_ask_queues_task_alias() {
        let dir = temp_dir("slash-ask");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/ask fix the parser".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.task_queue.len(), 1, "{:?}", app.task_queue);
        assert_eq!(app.task_queue[0].text, "fix the parser");
        assert!(app.status.contains("Queued task"), "{}", app.status);
        let tasks = list_tasks(&dir).unwrap();
        assert_eq!(tasks.len(), 1, "{tasks:?}");
        assert_eq!(tasks[0].text, "fix the parser");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_agent_coding_aliases_queue_tasks() {
        for alias in ["/edit", "/fix", "/implement"] {
            let dir = temp_dir(&format!("slash-agent-alias-{}", &alias[1..]));
            std::fs::create_dir_all(&dir).unwrap();
            let mut app = app_with_root(dir.clone());
            let command = format!("{alias} improve the login form");

            for c in command.chars() {
                handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
            }
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

            assert_eq!(app.task_queue.len(), 1, "{alias}: {:?}", app.task_queue);
            assert_eq!(app.task_queue[0].text, "improve the login form");
            assert!(
                app.status.contains("Queued task"),
                "{alias}: {}",
                app.status
            );
            let tasks = list_tasks(&dir).unwrap();
            assert_eq!(tasks.len(), 1, "{alias}: {tasks:?}");
            assert_eq!(tasks[0].text, "improve the login form");

            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn slash_doctor_health_compat_show_agent_compatibility() {
        for alias in ["/doctor", "/health", "/compat"] {
            let dir = temp_dir(&format!("slash-{}", &alias[1..]));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("CLAUDE.md"), "claude rules\n").unwrap();
            std::fs::write(dir.join(".aider.conf.yml"), "model: test\n").unwrap();
            let mut app = app_with_root(dir.clone());

            for c in alias.chars() {
                handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
            }
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

            let terminal = app.terminal_log.join("\n");
            assert_eq!(app.active_pane, WorkbenchPane::Terminal, "{alias}");
            assert!(
                terminal.contains("Agent compatibility"),
                "{alias}: {terminal}"
            );
            assert!(terminal.contains("Claude Code"), "{alias}: {terminal}");
            assert!(
                terminal.contains("Aider config detected"),
                "{alias}: {terminal}"
            );
            assert!(
                app.status.contains("Agent compatibility"),
                "{alias}: {}",
                app.status
            );

            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn slash_logs_and_trace_focus_familiar_views() {
        let dir = temp_dir("slash-views");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir);

        for c in "/logs".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(app.status.contains("Terminal"), "{}", app.status);

        for c in "/trace".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(app.active_pane, WorkbenchPane::Trace);
        assert!(app.status.contains("Trace"), "{}", app.status);
    }

    #[test]
    fn slash_plan_next_and_done_drive_planning_engine() {
        let dir = temp_dir("slash-plan");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/plan ship planning engine".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());
        assert!(
            app.plan_status.contains("ship planning engine"),
            "{}",
            app.plan_status
        );
        assert!(app.status.contains("Plan created"), "{}", app.status);

        for c in "/next".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());
        assert_eq!(app.task_queue.len(), 1, "{:?}", app.task_queue);
        assert!(app.task_queue[0].text.contains("ship planning engine"));
        assert!(
            app.plan_status.contains("[queued] step-1"),
            "{}",
            app.plan_status
        );

        for c in "/done cargo test passed".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());
        assert!(app.status.contains("Plan step completed"), "{}", app.status);
        assert!(
            app.plan_status.contains("[done] step-1"),
            "{}",
            app.plan_status
        );
        assert!(
            app.plan_status.contains("cargo test passed"),
            "{}",
            app.plan_status
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_launch_shows_readiness_checklist() {
        let dir = temp_dir("slash-launch");
        std::fs::create_dir_all(dir.join(".github/workflows")).unwrap();
        std::fs::write(dir.join(".github/workflows/ci.yml"), "name: CI\n").unwrap();
        std::fs::write(dir.join("README.md"), "# Demo\n").unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/launch".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        let terminal = app.terminal_log.join("\n");
        assert!(terminal.contains("Launch Readiness"), "{terminal}");
        assert!(terminal.contains("[ready] CI workflow"), "{terminal}");
        assert!(
            terminal.contains("[missing] Benchmark result"),
            "{terminal}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_diff_refreshes_preview_without_queueing_task() {
        let dir = temp_dir("slash-diff");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        let mut app = app_with_root(dir.clone());
        app.diff_preview = "stale".into();
        std::fs::write(dir.join("slash.txt"), "slash\n").unwrap();

        for c in "/diff".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Session);
        assert!(
            app.status.contains("Diff preview refreshed"),
            "{}",
            app.status
        );
        assert!(
            app.diff_preview.contains("slash.txt"),
            "{}",
            app.diff_preview
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_provider_updates_deepseek_profile_and_writes_config() {
        let dir = temp_dir("slash-provider");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/provider deepseek deepseek-reasoner".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.config.provider.default_provider, "openai");
        assert_eq!(app.config.provider.default_model, "deepseek-reasoner");
        assert_eq!(
            app.config.provider.base_url.as_deref(),
            Some("https://api.deepseek.com")
        );
        assert_eq!(
            app.config.provider.api_key_env.as_deref(),
            Some("DEEPSEEK_API_KEY")
        );
        assert!(app.status.contains("Provider updated"), "{}", app.status);

        let saved = ArgusCodeConfig::read(&dir).unwrap();
        assert_eq!(saved.provider, app.config.provider);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_model_updates_current_model_and_writes_config() {
        let dir = temp_dir("slash-model");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/model kimi-k2".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.config.provider.default_provider, "mock");
        assert_eq!(app.config.provider.default_model, "kimi-k2");
        assert!(app.status.contains("Model updated"), "{}", app.status);

        let saved = ArgusCodeConfig::read(&dir).unwrap();
        assert_eq!(saved.provider.default_model, "kimi-k2");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_cancel_marks_task_canceled_without_running_it() {
        let dir = temp_dir("slash-cancel");
        std::fs::create_dir_all(&dir).unwrap();
        let task = queue_task(&dir, "delete generated files").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        for c in format!("/cancel {}", task.id).chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.task_queue[0].status, "canceled");
        assert_eq!(list_tasks(&dir).unwrap()[0].status, "canceled");
        assert!(app.status.contains("Task canceled"), "{}", app.status);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_stop_requests_active_background_cancel() {
        let dir = temp_dir("slash-stop");
        std::fs::create_dir_all(&dir).unwrap();
        let task = queue_task(&dir, "long running edit").unwrap();
        crate::background::record_background_run(
            &dir,
            &task.id,
            "running",
            "task is still running",
            Some(PathBuf::from(".argus/tasks/long.trace.jsonl")),
        )
        .unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        for c in "/stop".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        let request = crate::background::load_background_cancel(&dir)
            .unwrap()
            .unwrap();
        let state = crate::background::load_background_run(&dir)
            .unwrap()
            .unwrap();

        assert_eq!(request.task_id, task.id);
        assert_eq!(state.status, "canceling");
        assert!(app.status.contains("Stop requested"), "{}", app.status);
        assert!(
            app.terminal_log
                .iter()
                .any(|line| line.contains("stop requested")),
            "{:?}",
            app.terminal_log
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_retry_requeues_task() {
        let dir = temp_dir("slash-retry");
        std::fs::create_dir_all(&dir).unwrap();
        let task = queue_task(&dir, "fix flaky test").unwrap();
        crate::tasks::update_task_status(&dir, &task.id, "failed").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        for c in format!("/retry {}", task.id).chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.task_queue[0].status, "queued");
        assert_eq!(list_tasks(&dir).unwrap()[0].status, "queued");
        assert!(app.status.contains("Task requeued"), "{}", app.status);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_sandbox_updates_security_profile_and_writes_config() {
        let dir = temp_dir("slash-sandbox");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/sandbox read-only".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.config.security.sandbox, "read-only");
        assert!(app.status.contains("Sandbox updated"), "{}", app.status);
        let saved = ArgusCodeConfig::read(&dir).unwrap();
        assert_eq!(saved.security.sandbox, "read-only");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_approval_updates_security_profile_and_writes_config() {
        let dir = temp_dir("slash-approval");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/approval ask".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.config.security.approval, "ask");
        assert!(app.status.contains("Approval updated"), "{}", app.status);
        let saved = ArgusCodeConfig::read(&dir).unwrap();
        assert_eq!(saved.security.approval, "ask");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workbench_loads_and_renders_repo_map() {
        let dir = temp_dir("repo-map");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn demo() {}\n").unwrap();
        std::fs::write(dir.join("AGENTS.md"), "# Rules\n").unwrap();
        let seed = app_with_root(dir.clone());

        let app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        assert!(app.repo_map.contains("Repo Map"), "{}", app.repo_map);
        assert!(app.repo_map.contains("src"), "{}", app.repo_map);
        assert!(app.repo_map.contains("rs"), "{}", app.repo_map);

        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Repo Map"), "{text}");
        assert!(text.contains("src"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_map_refreshes_repo_map_without_queueing_task() {
        let dir = temp_dir("slash-map");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();
        app.repo_map = "stale".into();
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::write(dir.join("tests/smoke.rs"), "#[test] fn smoke() {}\n").unwrap();

        for c in "/map".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Project);
        assert!(app.status.contains("Repo map refreshed"), "{}", app.status);
        assert!(app.repo_map.contains("tests"), "{}", app.repo_map);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workbench_loads_and_renders_eval_dashboard() {
        let dir = temp_dir("eval-dashboard");
        std::fs::create_dir_all(dir.join(".argus/evals")).unwrap();
        std::fs::write(
            dir.join(".argus/evals/smoke.json"),
            r#"{"name":"demo smoke","cases":[{"id":"smoke","task":"check","verify":["cargo test"]}]}"#,
        )
        .unwrap();
        let seed = app_with_root(dir.clone());

        let app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        assert!(
            app.eval_dashboard.contains("Eval Dashboard"),
            "{}",
            app.eval_dashboard
        );
        assert!(
            app.eval_dashboard.contains("demo smoke"),
            "{}",
            app.eval_dashboard
        );
        assert!(
            app.eval_dashboard.contains("smoke"),
            "{}",
            app.eval_dashboard
        );

        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Eval Dashboard"), "{text}");
        assert!(text.contains("demo smoke"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_evals_refreshes_dashboard_without_queueing_task() {
        let dir = temp_dir("slash-evals");
        std::fs::create_dir_all(dir.join(".argus/evals")).unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();
        app.eval_dashboard = "stale".into();
        std::fs::write(
            dir.join(".argus/evals/regression.json"),
            r#"{"name":"regression","cases":[{"id":"case-a","task":"fix","verify":["true"]}]}"#,
        )
        .unwrap();

        for c in "/evals".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Trace);
        assert!(
            app.status.contains("Eval dashboard refreshed"),
            "{}",
            app.status
        );
        assert!(
            app.eval_dashboard.contains("regression"),
            "{}",
            app.eval_dashboard
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_eval_run_defaults_to_smoke_suite_without_queueing_task() {
        let dir = temp_dir("slash-eval-run");
        std::fs::create_dir_all(dir.join(".argus/evals")).unwrap();
        std::fs::write(
            dir.join(".argus/evals/smoke.json"),
            r#"{"name":"demo smoke","cases":[{"id":"smoke","task":"check","verify":["true"]}]}"#,
        )
        .unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();
        app.eval_dashboard = "stale".into();

        let mut task_runner = |_root: &Path, _task: &TaskRecord| -> Result<HarnessRunOutput> {
            panic!("eval-run must not invoke the task harness")
        };
        let mut seen_suite = None;
        let mut eval_runner =
            |root: &Path, config: &ArgusCodeConfig, suite: &Path| -> Result<EvalRunOutput> {
                assert_eq!(root, dir.as_path());
                assert_eq!(config.provider.default_provider, "mock");
                seen_suite = Some(suite.to_path_buf());
                Ok(EvalRunOutput {
                    suite: suite.to_path_buf(),
                    out_dir: PathBuf::from(".argus/eval-runs"),
                    report_json: PathBuf::from(".argus/eval-runs/smoke.report.json"),
                    status: "passed".into(),
                    stdout: "eval: demo smoke\n1/1 passed (100%)\n".into(),
                    stderr: String::new(),
                })
            };
        let mut route_runner =
            |_root: &Path,
             _task: &TaskRecord,
             _cheap: &str,
             _strong: &str|
             -> Result<RouteRunOutput> { panic!("eval-run must not invoke route") };

        app.execute_slash_command_with(
            "/eval-run",
            &mut task_runner,
            &mut eval_runner,
            &mut route_runner,
        );

        assert_eq!(
            seen_suite.unwrap(),
            PathBuf::from(".argus/evals/smoke.json")
        );
        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(app.status.contains("Eval passed"), "{}", app.status);
        let terminal = app.terminal_log.join("\n");
        assert!(
            terminal.contains("$ argus eval .argus/evals/smoke.json"),
            "{terminal}"
        );
        assert!(terminal.contains("eval: demo smoke"), "{terminal}");
        assert!(
            terminal.contains("report: .argus/eval-runs/smoke.report.json"),
            "{terminal}"
        );
        assert!(
            app.eval_dashboard.contains("demo smoke"),
            "{}",
            app.eval_dashboard
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_route_run_executes_latest_task_with_explicit_models() {
        let dir = temp_dir("slash-route-run");
        std::fs::create_dir_all(&dir).unwrap();
        let task = queue_task(&dir, "fix route regression").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        let mut task_runner = |_root: &Path, _task: &TaskRecord| -> Result<HarnessRunOutput> {
            panic!("route-run must not invoke the direct task harness")
        };
        let mut eval_runner =
            |_root: &Path, _config: &ArgusCodeConfig, _suite: &Path| -> Result<EvalRunOutput> {
                panic!("route-run must not invoke eval")
            };
        let mut seen = None;
        let mut route_runner =
            |root: &Path, task: &TaskRecord, cheap: &str, strong: &str| -> Result<RouteRunOutput> {
                seen = Some((task.id.clone(), cheap.to_string(), strong.to_string()));
                crate::tasks::update_task_status(root, &task.id, "done").unwrap();
                Ok(RouteRunOutput {
                    task_id: task.id.clone(),
                    task_text: task.text.clone(),
                    status: "done".into(),
                    trace: PathBuf::from(".argus/tasks/route.trace.jsonl"),
                    cheap_model: cheap.into(),
                    strong_model: strong.into(),
                    stdout: "route: escalated cheap-model -> strong-model (passed)\n".into(),
                    stderr: "(trace written to .argus/tasks/route.trace.jsonl)\n".into(),
                })
            };

        app.execute_slash_command_with(
            "/route-run cheap-model strong-model",
            &mut task_runner,
            &mut eval_runner,
            &mut route_runner,
        );

        assert_eq!(
            seen,
            Some((
                task.id,
                "cheap-model".to_string(),
                "strong-model".to_string()
            ))
        );
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert_eq!(app.task_queue[0].status, "done");
        assert!(app.status.contains("Route task"), "{}", app.status);
        let terminal = app.terminal_log.join("\n");
        assert!(
            terminal.contains("$ argus route"),
            "route command missing: {terminal}"
        );
        assert!(
            terminal.contains("--cheap cheap-model --strong strong-model"),
            "models missing: {terminal}"
        );
        assert!(
            terminal.contains("route: escalated cheap-model -> strong-model"),
            "stdout missing: {terminal}"
        );
        assert!(
            terminal.contains("trace: .argus/tasks/route.trace.jsonl"),
            "trace missing: {terminal}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_remember_appends_lesson_and_refreshes_memory_preview() {
        let dir = temp_dir("slash-remember");
        std::fs::create_dir_all(dir.join(".argus/memory")).unwrap();
        std::fs::write(
            dir.join(".argus/memory/project.md"),
            "# Project\n\nRust CLI.\n",
        )
        .unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        for c in "/remember Always run clippy before release".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Trace);
        assert!(app.status.contains("Lesson remembered"), "{}", app.status);
        assert!(
            app.memory_preview
                .contains("Always run clippy before release"),
            "{}",
            app.memory_preview
        );
        let lessons = std::fs::read_to_string(dir.join(".argus/memory/lessons.md")).unwrap();
        assert!(
            lessons.contains("- Always run clippy before release"),
            "{lessons}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_mcp_updates_server_and_allowlist_config() {
        let dir = temp_dir("slash-mcp");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/mcp argus __mcp-mock".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.config.mcp.command.as_deref(), Some("argus __mcp-mock"));
        assert!(app.status.contains("MCP profile updated"), "{}", app.status);

        for c in "/mcp-allow echo".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.config.mcp.allow, vec!["echo"]);
        let saved = ArgusCodeConfig::read(&dir).unwrap();
        assert_eq!(saved.mcp.command.as_deref(), Some("argus __mcp-mock"));
        assert_eq!(saved.mcp.allow, vec!["echo"]);
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        let terminal = app.terminal_log.join("\n");
        assert!(terminal.contains("mcp: argus __mcp-mock"), "{terminal}");
        assert!(terminal.contains("allow: echo"), "{terminal}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_checkpoint_and_rollback_restore_workspace_files() {
        let dir = temp_dir("slash-checkpoint");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "before\n").unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/checkpoint before risky edit".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());
        assert!(app.status.contains("Checkpoint saved"), "{}", app.status);
        let checkpoint = crate::checkpoints::latest_checkpoint(&dir)
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint.label, "before risky edit");

        std::fs::write(dir.join("src/lib.rs"), "after\n").unwrap();
        std::fs::write(dir.join("src/new.rs"), "new\n").unwrap();
        for c in "/rollback".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(
            std::fs::read_to_string(dir.join("src/lib.rs")).unwrap(),
            "before\n"
        );
        assert!(!dir.join("src/new.rs").exists());
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(app.status.contains("Rolled back"), "{}", app.status);
        assert!(
            app.terminal_log.join("\n").contains(&checkpoint.id),
            "{:?}",
            app.terminal_log
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_review_refreshes_change_review_without_queueing_task() {
        let dir = temp_dir("slash-review");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("new-file.txt"), "hello\n").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();
        app.change_review = "stale".into();

        for c in "/review".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert!(list_tasks(&dir).unwrap().is_empty());
        assert_eq!(app.active_pane, WorkbenchPane::Session);
        assert!(
            app.status.contains("Change review refreshed"),
            "{}",
            app.status
        );
        assert!(
            app.change_review.contains("Change Review"),
            "{}",
            app.change_review
        );
        assert!(
            app.change_review.contains("new-file.txt"),
            "{}",
            app.change_review
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_patch_refreshes_change_review_alias() {
        let dir = temp_dir("slash-patch");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        let mut app = app_with_root(dir.clone());
        app.change_review = "stale".into();
        std::fs::write(dir.join("patch.txt"), "patch\n").unwrap();

        for c in "/patch".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert!(app.task_queue.is_empty(), "{:?}", app.task_queue);
        assert_eq!(app.active_pane, WorkbenchPane::Session);
        assert!(
            app.change_review.contains("Changed files"),
            "{}",
            app.change_review
        );
        assert!(
            app.change_review.contains("patch.txt"),
            "{}",
            app.change_review
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_accept_records_review_decision() {
        let dir = temp_dir("slash-accept");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/accept looks good".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert!(app.status.contains("Review accepted"), "{}", app.status);
        let decisions =
            std::fs::read_to_string(dir.join(".argus/reviews/decisions.jsonl")).unwrap();
        assert!(
            decisions.contains("\"decision\":\"accepted\""),
            "{decisions}"
        );
        assert!(decisions.contains("\"note\":\"looks good\""), "{decisions}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_rework_queues_follow_up_task() {
        let dir = temp_dir("slash-rework");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());

        for c in "/rework tighten parser edge cases".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.active_pane, WorkbenchPane::Session);
        assert_eq!(app.task_queue.len(), 1);
        assert!(
            app.task_queue[0]
                .text
                .contains("Review follow-up: tighten parser edge cases"),
            "{:?}",
            app.task_queue
        );
        let tasks = list_tasks(&dir).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(app.status.contains("Rework queued"), "{}", app.status);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_latest_task_creates_checkpoint_before_runner() {
        let dir = temp_dir("auto-checkpoint-run");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.rs"), "before\n").unwrap();
        queue_task(&dir, "rewrite main").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        app.run_latest_task_with(&mut |root, task| {
            let checkpoint = crate::checkpoints::latest_checkpoint(root)
                .unwrap()
                .unwrap();
            assert!(checkpoint.label.contains(&task.id), "{}", checkpoint.label);
            std::fs::write(root.join("main.rs"), "after\n").unwrap();
            crate::tasks::update_task_status(root, &task.id, "done").unwrap();
            Ok(HarnessRunOutput {
                task_id: task.id.clone(),
                task_text: task.text.clone(),
                status: "done".into(),
                trace: PathBuf::from(".argus/tasks/fake.trace.jsonl"),
                stdout: "changed file".into(),
                stderr: String::new(),
            })
        })
        .unwrap();

        let checkpoint = crate::checkpoints::latest_checkpoint(&dir)
            .unwrap()
            .unwrap();
        crate::checkpoints::restore_checkpoint(&dir, &checkpoint.id).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("main.rs")).unwrap(),
            "before\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_failure_marks_task_failed_and_queues_repair_task() {
        let dir = temp_dir("run-failure-repair");
        std::fs::create_dir_all(&dir).unwrap();
        let original = queue_task(&dir, "implement parser recovery").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        let result =
            app.run_latest_task_with(&mut |_root, _task| Err(anyhow::anyhow!("model crashed")));

        assert!(result.is_err());
        let tasks = list_tasks(&dir).unwrap();
        assert_eq!(tasks.len(), 2, "{tasks:?}");
        assert_eq!(tasks[0].id, original.id);
        assert_eq!(tasks[0].status, "failed");
        assert!(
            tasks[1].text.contains("Repair harness run failure"),
            "{tasks:?}"
        );
        assert!(
            tasks[1].text.contains("implement parser recovery"),
            "{tasks:?}"
        );
        assert!(tasks[1].text.contains("model crashed"), "{tasks:?}");
        assert!(app.status.contains("Repair task queued"), "{}", app.status);
        assert!(
            app.terminal_log.join("\n").contains("repair:"),
            "{:?}",
            app.terminal_log
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workbench_starts_latest_task_in_background_and_refreshes_completion() {
        let dir = temp_dir("background-run");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            "{\"scripts\":{\"test\":\"echo ok\"}}\n",
        )
        .unwrap();
        let record = queue_task(&dir, "run without blocking the tui").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        app.start_latest_task_background_with(|root, task| {
            std::thread::sleep(std::time::Duration::from_millis(60));
            crate::tasks::update_task_status(&root, &task.id, "done").unwrap();
            crate::cockpit::append_cockpit_event(
                &root,
                "harness",
                &format!("task {} completed with status done", task.id),
                "/review",
            )
            .unwrap();
            Ok(HarnessRunOutput {
                task_id: task.id.clone(),
                task_text: task.text.clone(),
                status: "done".into(),
                trace: PathBuf::from(".argus/tasks/background.trace.jsonl"),
                stdout: "background output".into(),
                stderr: String::new(),
            })
        })
        .unwrap();

        let started = crate::background::load_background_run(&dir)
            .unwrap()
            .unwrap();
        assert_eq!(started.task_id, record.id);
        assert!(
            matches!(started.status.as_str(), "running" | "done"),
            "{started:?}"
        );
        assert!(
            app.status.contains("Background run started"),
            "{}",
            app.status
        );

        let mut completed = None;
        for _ in 0..40 {
            app.tick();
            completed = crate::background::load_background_run(&dir).unwrap();
            if completed
                .as_ref()
                .is_some_and(|state| state.status == "done")
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let completed = completed.expect("background run status should exist");
        assert_eq!(completed.status, "done", "{completed:?}");
        assert!(
            app.cockpit_journal.contains("completed with status done"),
            "{}",
            app.cockpit_journal
        );
        assert!(app.status.contains("Background run done"), "{}", app.status);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_failure_marks_task_failed_and_queues_repair_task() {
        let dir = temp_dir("background-failure-repair");
        std::fs::create_dir_all(&dir).unwrap();
        let original = queue_task(&dir, "repair background failure").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        app.start_latest_task_background_with(|_root, _task| {
            Err(anyhow::anyhow!("background crashed"))
        })
        .unwrap();

        for _ in 0..40 {
            app.tick();
            let state = crate::background::load_background_run(&dir).unwrap();
            if state.as_ref().is_some_and(|state| state.status == "failed") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        app.tick();

        let tasks = list_tasks(&dir).unwrap();
        assert_eq!(tasks.len(), 2, "{tasks:?}");
        assert_eq!(tasks[0].id, original.id);
        assert_eq!(tasks[0].status, "failed");
        assert!(
            tasks[1].text.contains("Repair harness run failure"),
            "{tasks:?}"
        );
        assert!(tasks[1].text.contains("background crashed"), "{tasks:?}");
        assert!(app.status.contains("Repair task queued"), "{}", app.status);
        assert!(
            app.terminal_log.join("\n").contains("repair queued"),
            "{:?}",
            app.terminal_log
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tick_appends_background_output_without_duplicates() {
        let dir = temp_dir("background-output");
        std::fs::create_dir_all(&dir).unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        crate::background::append_background_output(&dir, "stdout", "first line\n").unwrap();
        crate::background::append_background_output(&dir, "stderr", "warn line\n").unwrap();

        app.tick();
        app.tick();

        let terminal = app.terminal_log.join("\n");
        assert!(terminal.contains("[stdout] first line"), "{terminal}");
        assert!(terminal.contains("[stderr] warn line"), "{terminal}");
        assert_eq!(
            app.terminal_log
                .iter()
                .filter(|line| line.contains("first line"))
                .count(),
            1,
            "{:?}",
            app.terminal_log
        );

        crate::background::clear_background_output(&dir).unwrap();
        crate::background::append_background_output(&dir, "stdout", "new run\n").unwrap();
        app.tick();

        let terminal = app.terminal_log.join("\n");
        assert!(terminal.contains("[stdout] new run"), "{terminal}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tick_refreshes_running_background_trace_without_status_change() {
        let dir = temp_dir("live-trace");
        let trace_rel = PathBuf::from(".argus/tasks/live.trace.jsonl");
        let trace_path = dir.join(&trace_rel);
        std::fs::create_dir_all(trace_path.parent().unwrap()).unwrap();
        let mut writer = TraceWriter::create(&trace_path).unwrap();
        writer
            .record(EventKind::TaskStarted {
                task: "live tail trace".into(),
            })
            .unwrap();
        crate::background::record_background_run(
            &dir,
            "task-live",
            "running",
            "task task-live running in background",
            Some(trace_rel.clone()),
        )
        .unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        app.tick();
        assert_eq!(app.latest_trace_path, Some(trace_rel.clone()));
        assert!(
            app.trace_preview
                .lines
                .iter()
                .any(|line| line.contains("live tail trace")),
            "{:?}",
            app.trace_preview
        );

        writer
            .record(EventKind::ToolCall {
                name: "shell".into(),
                args: "{\"cmd\":\"cargo test\"}".into(),
            })
            .unwrap();
        app.tick();

        assert!(
            app.trace_preview
                .lines
                .iter()
                .any(|line| line.contains("TOOL") && line.contains("shell")),
            "{:?}",
            app.trace_preview
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_completion_preserves_canceled_status() {
        let dir = temp_dir("background-canceled");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            "{\"scripts\":{\"test\":\"echo ok\"}}\n",
        )
        .unwrap();
        let record = queue_task(&dir, "cancel me").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        app.start_latest_task_background_with(|root, task| {
            crate::tasks::update_task_status(&root, &task.id, "canceled").unwrap();
            Ok(HarnessRunOutput {
                task_id: task.id.clone(),
                task_text: task.text.clone(),
                status: "canceled".into(),
                trace: PathBuf::from(".argus/tasks/canceled.trace.jsonl"),
                stdout: "stopped".into(),
                stderr: String::new(),
            })
        })
        .unwrap();

        let mut state = None;
        for _ in 0..40 {
            app.tick();
            state = crate::background::load_background_run(&dir).unwrap();
            if state
                .as_ref()
                .is_some_and(|state| state.status == "canceled")
                && app.status.contains("Background run canceled")
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let state = state.expect("background run status should exist");

        assert_eq!(state.task_id, record.id);
        assert_eq!(state.status, "canceled", "{state:?}");
        assert!(
            app.status.contains("Background run canceled"),
            "{}",
            app.status
        );
        assert_eq!(list_tasks(&dir).unwrap()[0].status, "canceled");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slash_run_default_path_starts_background_run_state() {
        let dir = temp_dir("slash-background-run");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            "{\"scripts\":{\"test\":\"echo ok\"}}\n",
        )
        .unwrap();
        let record = queue_task(&dir, "start from slash run").unwrap();
        let seed = app_with_root(dir.clone());
        let mut app = WorkbenchApp::load(seed.profile, seed.config).unwrap();

        for c in "/run".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::empty());
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::empty());

        let state = crate::background::load_background_run(&dir)
            .unwrap()
            .unwrap();
        assert_eq!(state.task_id, record.id);
        assert!(
            matches!(state.status.as_str(), "running" | "done"),
            "{state:?}"
        );
        assert!(
            app.status.contains("Background run started"),
            "{}",
            app.status
        );

        for _ in 0..40 {
            let state = crate::background::load_background_run(&dir).unwrap();
            if state
                .as_ref()
                .is_some_and(|state| state.status != "running")
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workbench_runs_latest_task_and_renders_harness_output() {
        let dir = temp_dir("run-task");
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = app_with_root(dir.clone());
        let record = queue_task(&dir, "fix the failing parser test").unwrap();
        app = WorkbenchApp::load(app.profile, app.config).unwrap();

        let mut ran_task = None;
        app.run_latest_task_with(&mut |root, task| {
            ran_task = Some(task.id.clone());
            crate::tasks::update_task_status(root, &task.id, "done").unwrap();
            Ok(HarnessRunOutput {
                task_id: task.id.clone(),
                task_text: task.text.clone(),
                status: "done".into(),
                trace: PathBuf::from(".argus/tasks/fake.trace.jsonl"),
                stdout: "model output".into(),
                stderr: "(trace written to .argus/tasks/fake.trace.jsonl)".into(),
            })
        })
        .unwrap();

        assert_eq!(ran_task, Some(record.id));
        assert_eq!(app.active_pane, WorkbenchPane::Terminal);
        assert_eq!(app.task_queue[0].status, "done");
        assert_eq!(
            app.latest_trace_path,
            Some(PathBuf::from(".argus/tasks/fake.trace.jsonl"))
        );
        assert!(
            app.terminal_log.join("\n").contains("model output"),
            "{:?}",
            app.terminal_log
        );

        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("model output"), "{text}");
        assert!(text.contains("fake.trace.jsonl"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workbench_loads_and_renders_session_history() {
        let dir = temp_dir("session-history");
        std::fs::create_dir_all(&dir).unwrap();
        let app = app_with_root(dir.clone());
        append_session(
            &dir,
            "task-1",
            "ship the history panel",
            "done",
            ".argus/tasks/task-1.trace.jsonl",
        )
        .unwrap();

        let app = WorkbenchApp::load(app.profile, app.config).unwrap();

        assert_eq!(app.session_history.len(), 1);
        assert_eq!(app.session_history[0].task_text, "ship the history panel");
        assert_eq!(
            app.latest_trace_path,
            Some(PathBuf::from(".argus/tasks/task-1.trace.jsonl"))
        );

        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Session History"), "{text}");
        assert!(text.contains("ship the history panel"), "{text}");
        assert!(text.contains("task-1.trace.jsonl"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workbench_loads_and_renders_latest_trace_events() {
        let dir = temp_dir("trace-events");
        let trace_path = dir.join(".argus/tasks/task-1.trace.jsonl");
        std::fs::create_dir_all(trace_path.parent().unwrap()).unwrap();
        let mut writer = TraceWriter::create(&trace_path).unwrap();
        writer
            .record(EventKind::TaskStarted {
                task: "repair parser".into(),
            })
            .unwrap();
        writer
            .record(EventKind::VerificationGate {
                passed: true,
                detail: "cargo test passed".into(),
            })
            .unwrap();

        let app = app_with_root(dir.clone());
        append_session(
            &dir,
            "task-1",
            "repair parser",
            "done",
            ".argus/tasks/task-1.trace.jsonl",
        )
        .unwrap();

        let app = WorkbenchApp::load(app.profile, app.config).unwrap();

        assert!(
            app.trace_preview
                .lines
                .iter()
                .any(|line| line.contains("TASK") && line.contains("repair parser")),
            "{:?}",
            app.trace_preview
        );
        assert!(
            app.trace_preview
                .lines
                .iter()
                .any(|line| line.contains("GATE") && line.contains("cargo test passed")),
            "{:?}",
            app.trace_preview
        );

        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Trace Timeline"), "{text}");
        assert!(text.contains("repair parser"), "{text}");
        assert!(text.contains("cargo test passed"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workbench_loads_and_renders_diff_preview() {
        let dir = temp_dir("diff-preview");
        std::fs::create_dir_all(&dir).unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("pending.txt"), "pending\n").unwrap();
        let app = app_with_root(dir.clone());

        let app = WorkbenchApp::load(app.profile, app.config).unwrap();

        assert!(
            app.diff_preview.contains("Git Status"),
            "{}",
            app.diff_preview
        );
        assert!(
            app.diff_preview.contains("?? pending.txt"),
            "{}",
            app.diff_preview
        );

        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Diff Preview"), "{text}");
        assert!(text.contains("pending.txt"), "{text}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

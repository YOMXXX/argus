use crate::config::{ArgusCodeConfig, CONFIG_PATH};
use crate::diff::load_diff_preview;
use crate::harness::{run_task_through_harness, HarnessRunOutput};
use crate::project::{detect_project, init_project, ProjectProfile};
use crate::sessions::{list_sessions, SessionRecord};
use crate::tasks::{latest_resumable_task, list_tasks, queue_task, TaskRecord};
use crate::trace_view::{load_trace_preview, TracePreview};
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
    Verify,
    Memory,
    History,
    RefreshDiff,
    SmokeEval,
    NewTask,
}

#[derive(Debug, Clone, Copy)]
struct PaletteItem {
    action: PaletteAction,
    label: &'static str,
    detail: &'static str,
}

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
        action: PaletteAction::SmokeEval,
        label: "Open smoke eval",
        detail: "Prepare the generated .argus/evals/smoke.json suite",
    },
    PaletteItem {
        action: PaletteAction::NewTask,
        label: "New coding task",
        detail: "Focus the conversation input",
    },
];

#[derive(Debug, Clone)]
pub struct WorkbenchApp {
    pub profile: ProjectProfile,
    pub config: ArgusCodeConfig,
    pub active_pane: WorkbenchPane,
    pub mode: WorkbenchMode,
    pub palette_selected: usize,
    pub task_queue: Vec<TaskRecord>,
    pub session_history: Vec<SessionRecord>,
    pub diff_preview: String,
    pub trace_preview: TracePreview,
    pub terminal_log: Vec<String>,
    pub latest_trace_path: Option<PathBuf>,
    pub input: String,
    pub status: String,
}

impl WorkbenchApp {
    pub fn new(profile: ProjectProfile, config: ArgusCodeConfig) -> Self {
        Self::with_state(
            profile,
            config,
            Vec::new(),
            Vec::new(),
            "(not loaded)".into(),
            TracePreview::empty(),
        )
    }

    pub fn load(profile: ProjectProfile, config: ArgusCodeConfig) -> Result<Self> {
        let task_queue = list_tasks(&profile.root)?;
        let session_history = list_sessions(&profile.root)?;
        let diff_preview = load_diff_preview(&profile.root)?;
        let latest_trace_path = session_history.last().map(|session| session.trace.clone());
        let trace_preview = load_trace_preview(&profile.root, latest_trace_path.as_deref());
        Ok(Self::with_state(
            profile,
            config,
            task_queue,
            session_history,
            diff_preview,
            trace_preview,
        ))
    }

    fn with_state(
        profile: ProjectProfile,
        config: ArgusCodeConfig,
        task_queue: Vec<TaskRecord>,
        session_history: Vec<SessionRecord>,
        diff_preview: String,
        trace_preview: TracePreview,
    ) -> Self {
        let latest_trace_path = session_history.last().map(|session| session.trace.clone());
        Self {
            profile,
            config,
            active_pane: WorkbenchPane::Session,
            mode: WorkbenchMode::Normal,
            palette_selected: 0,
            task_queue,
            session_history,
            diff_preview,
            trace_preview,
            terminal_log: Vec::new(),
            latest_trace_path,
            input: String::new(),
            status: "Ready. Type a task, Tab switches panes, Ctrl+K opens command palette.".into(),
        }
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
        } else {
            match queue_task(&self.profile.root, &task) {
                Ok(record) => {
                    self.status = format!("Queued task {}: {}", record.id, record.text);
                    self.task_queue.push(record);
                    self.input.clear();
                }
                Err(err) => {
                    self.status = format!("Could not queue task: {err}");
                }
            }
        }
    }

    fn open_palette(&mut self) {
        self.mode = WorkbenchMode::CommandPalette;
        self.palette_selected = 0;
        self.status = "Command palette open. Up/Down select, Enter run, Esc close.".into();
    }

    fn close_overlay(&mut self) {
        self.mode = WorkbenchMode::Normal;
        self.status = "Ready.".into();
    }

    fn palette_next(&mut self) {
        self.palette_selected = (self.palette_selected + 1) % PALETTE_ITEMS.len();
    }

    fn palette_prev(&mut self) {
        self.palette_selected = if self.palette_selected == 0 {
            PALETTE_ITEMS.len() - 1
        } else {
            self.palette_selected - 1
        };
    }

    fn execute_palette_action(&mut self) {
        let action = PALETTE_ITEMS[self.palette_selected].action;
        self.mode = WorkbenchMode::Normal;
        match action {
            PaletteAction::RunLatestTask => {
                if let Err(err) = self.run_latest_task_with(&mut run_task_through_harness) {
                    self.active_pane = WorkbenchPane::Terminal;
                    self.status = format!("Harness run failed: {err}");
                    self.terminal_log.push(format!("error: {err}"));
                }
            }
            PaletteAction::Verify => {
                self.active_pane = WorkbenchPane::Terminal;
                let commands = if self.config.verify.commands.is_empty() {
                    "no verify command configured".into()
                } else {
                    self.config.verify.commands.join(" && ")
                };
                self.status = format!("Ready to verify: {commands}");
            }
            PaletteAction::Memory => {
                self.active_pane = WorkbenchPane::Trace;
                self.status = format!("Memory opened: {}", self.config.memory.project);
            }
            PaletteAction::History => {
                self.active_pane = WorkbenchPane::Trace;
                self.status = format!(
                    "Session history opened: {} run(s)",
                    self.session_history.len()
                );
            }
            PaletteAction::RefreshDiff => {
                self.active_pane = WorkbenchPane::Session;
                match load_diff_preview(&self.profile.root) {
                    Ok(preview) => {
                        self.diff_preview = preview;
                        self.status = "Diff preview refreshed.".into();
                    }
                    Err(err) => {
                        self.status = format!("Could not refresh diff preview: {err}");
                    }
                }
            }
            PaletteAction::SmokeEval => {
                self.active_pane = WorkbenchPane::Trace;
                self.status = "Smoke eval ready: argus eval .argus/evals/smoke.json".into();
            }
            PaletteAction::NewTask => {
                self.active_pane = WorkbenchPane::Session;
                self.status = "New task ready. Type in the conversation input.".into();
            }
        }
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

        self.active_pane = WorkbenchPane::Terminal;
        self.status = format!("Running task {} through Argus harness...", task.id);
        self.terminal_log = vec![format!("$ arguscode resume --run  # {}", task.text)];

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
                self.trace_preview =
                    load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
                self.status = format!("Task {} {}", output.task_id, output.status);
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
                self.trace_preview =
                    load_trace_preview(&self.profile.root, self.latest_trace_path.as_deref());
                self.status = format!("Harness run failed: {err}");
                self.terminal_log.push(format!("error: {err}"));
                Err(err)
            }
        }
    }
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
            KeyCode::Down | KeyCode::Char('j') => app.palette_next(),
            KeyCode::Up | KeyCode::Char('k') if modifiers.is_empty() => app.palette_prev(),
            KeyCode::Enter => app.execute_palette_action(),
            KeyCode::Char('k') if modifiers.contains(KeyModifiers::CONTROL) => app.close_overlay(),
            KeyCode::Char('q') if modifiers.is_empty() => return false,
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
            KeyCode::Tab => app.next_pane(),
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
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    _ if !handle_key(app, key.code, key.modifiers) => break,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

pub fn ui(f: &mut Frame, app: &WorkbenchApp) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(6),
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
            .take(5)
            .map(|task| format!("[{}] {}  {}", task.status, task.id, task.text))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let text = format!(
        "Chat\n> {}\n\nTask Queue\n{}\n\nPlan\n1. Understand the request and repo rules.\n2. Edit through the harness.\n3. Run verification gate.\n4. Record trace and summarize evidence.\n\nDiff Preview\n{}\n\nVerify Profile\n{}",
        if app.input.is_empty() {
            "Type a task here, then press Enter.".to_string()
        } else {
            app.input.clone()
        },
        queue,
        app.diff_preview,
        verify
    );
    f.render_widget(
        Paragraph::new(text)
            .block(panel_block(
                "Conversation / Plan / Diff",
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
    lines.push(Line::from("Memory"));
    lines.push(Line::from(app.config.memory.project.clone()));
    lines.push(Line::from(""));
    lines.push(Line::from("Session History"));
    if app.session_history.is_empty() {
        lines.push(Line::from("(empty)"));
    } else {
        for session in app.session_history.iter().rev().take(4) {
            lines.push(Line::from(format!(
                "[{}] {}",
                session.status, session.task_text
            )));
            lines.push(Line::from(session.trace.display().to_string()));
        }
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
    let commands = if !app.terminal_log.is_empty() {
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
    f.render_widget(
        Paragraph::new(commands)
            .block(panel_block(
                "Terminal / Verify",
                app.active_pane == WorkbenchPane::Terminal,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_status(f: &mut Frame, app: &WorkbenchApp, area: Rect) {
    let status = format!(
        " {} · Tab pane · Enter queue · Esc/q quit · config {} ",
        app.status, CONFIG_PATH
    );
    f.render_widget(Paragraph::new(status), area);
}

fn render_command_palette(f: &mut Frame, app: &WorkbenchApp) {
    let area = centered_rect(66, 52, f.area());
    f.render_widget(Clear, area);
    let items = PALETTE_ITEMS
        .iter()
        .map(|item| ListItem::new(format!("{}  -  {}", item.label, item.detail)))
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select(Some(app.palette_selected));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title("Command Palette"),
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

fn render_help(f: &mut Frame) {
    let area = centered_rect(62, 48, f.area());
    f.render_widget(Clear, area);
    let text = "ArgusCode Help\n\n\
Ctrl+K  Open command palette\n\
Tab     Switch pane\n\
Enter   Queue task / run selected command\n\
?       Toggle help\n\
Esc     Close overlay or exit\n\
q       Quit\n\n\
Harness flow\n\
plan -> edit -> verify -> repair -> trace";
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
    use crate::harness::HarnessRunOutput;
    use crate::project::build_config;
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
    fn command_palette_opens_and_executes_verify_action() {
        let mut app = app();

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
        assert!(app.status.contains("Ready to verify"), "{}", app.status);
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

use crate::config::{ArgusCodeConfig, CONFIG_PATH};
use crate::project::{detect_project, init_project, ProjectProfile};
use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchPane {
    Project,
    Session,
    Trace,
    Terminal,
}

#[derive(Debug, Clone)]
pub struct WorkbenchApp {
    pub profile: ProjectProfile,
    pub config: ArgusCodeConfig,
    pub active_pane: WorkbenchPane,
    pub input: String,
    pub status: String,
}

impl WorkbenchApp {
    pub fn new(profile: ProjectProfile, config: ArgusCodeConfig) -> Self {
        Self {
            profile,
            config,
            active_pane: WorkbenchPane::Session,
            input: String::new(),
            status: "Ready. Type a task, Tab switches panes, Ctrl+K opens command palette soon."
                .into(),
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
        let task = self.input.trim();
        if task.is_empty() {
            self.status = "Enter a task to start an ArgusCode session.".into();
        } else {
            self.status = format!(
                "Queued task: {task}. Full live agent execution lands in the next harness milestone."
            );
            self.input.clear();
        }
    }
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
    let mut app = WorkbenchApp::new(profile, config);

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
                    KeyCode::Char('q') if key.modifiers.is_empty() => break,
                    KeyCode::Esc => break,
                    KeyCode::Tab => app.next_pane(),
                    KeyCode::Enter => app.submit_input(),
                    KeyCode::Backspace => app.pop_input(),
                    KeyCode::Char(c) => app.push_input(c),
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
        ListItem::new("Tasks"),
        ListItem::new("● active session"),
        ListItem::new("○ verify workspace"),
        ListItem::new("○ eval smoke"),
    ];
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
    let text = format!(
        "Chat\n> {}\n\nPlan\n1. Understand the request and repo rules.\n2. Edit through the harness.\n3. Run verification gate.\n4. Record trace and summarize evidence.\n\nDiff Preview\n(no pending diff)\n\nVerify Profile\n{}",
        if app.input.is_empty() {
            "Type a task here, then press Enter.".to_string()
        } else {
            app.input.clone()
        },
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
    let trace_path = PathBuf::from(".argus/trace.jsonl");
    let lines = vec![
        Line::from("step 000  TASK"),
        Line::from("step 001  PLAN"),
        Line::from("step 002  TOOL"),
        Line::from("step 003  GATE"),
        Line::from(""),
        Line::from("Memory"),
        Line::from(app.config.memory.project.clone()),
        Line::from(""),
        Line::from("Trace target"),
        Line::from(trace_path.display().to_string()),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(panel_block(
                "Trace / Memory",
                app.active_pane == WorkbenchPane::Trace,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_terminal(f: &mut Frame, app: &WorkbenchApp, area: Rect) {
    let commands = if app.config.verify.commands.is_empty() {
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
    use crate::project::build_config;
    use ratatui::backend::TestBackend;

    fn app() -> WorkbenchApp {
        let profile = ProjectProfile {
            root: PathBuf::from("/tmp/demo"),
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
}

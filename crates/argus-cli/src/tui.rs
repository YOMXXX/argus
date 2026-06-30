//! Argus TUI —— 双栏 Trace 浏览器(右时间线、左详情、底状态)。
//!
//! Elm 式拆分:App(状态)/ ui(渲染,可 TestBackend 测)/ run_tui(event loop)。

use anyhow::Result;
use argus_trace::{read_trace, EventKind, TraceEvent};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use std::path::Path;

/// TUI 应用状态。
pub struct App {
    pub events: Vec<TraceEvent>,
    pub selected: usize,
}

impl App {
    pub fn new(events: Vec<TraceEvent>) -> Self {
        Self {
            events,
            selected: 0,
        }
    }
    pub fn select_next(&mut self) {
        if !self.events.is_empty() && self.selected + 1 < self.events.len() {
            self.selected += 1;
        }
    }
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }
    pub fn selected_event(&self) -> Option<&TraceEvent> {
        self.events.get(self.selected)
    }
}

/// 时间线一行的简短摘要。
pub fn event_summary(e: &TraceEvent) -> String {
    let tag = match &e.kind {
        EventKind::TaskStarted { .. } => "TASK",
        EventKind::Thought { .. } => "THOUGHT",
        EventKind::ModelRequest { .. } => "MODEL->",
        EventKind::ModelResponse { .. } => "MODEL<-",
        EventKind::ToolCall { .. } => "TOOL->",
        EventKind::ToolResult { .. } => "TOOL<-",
        EventKind::PolicyDecision { .. } => "POLICY",
        EventKind::Diff { .. } => "DIFF",
        EventKind::VerificationGate { .. } => "GATE",
        EventKind::RouteDecision { .. } => "ROUTE",
        EventKind::Note { .. } => "NOTE",
    };
    format!("[{:>3}] {}", e.step, tag)
}

/// 选中 step 的详细内容(左栏详情用)。
pub fn event_detail(e: &TraceEvent) -> String {
    match &e.kind {
        EventKind::TaskStarted { task } => format!("TASK STARTED\n\n{task}"),
        EventKind::Thought { text } => format!("THOUGHT\n\n{text}"),
        EventKind::ModelRequest {
            model,
            prompt_tokens,
        } => {
            format!("MODEL REQUEST\n\nmodel: {model}\nprompt tokens: {prompt_tokens}")
        }
        EventKind::ModelResponse {
            model,
            prompt_tokens,
            completion_tokens,
            text,
        } => {
            format!("MODEL RESPONSE\n\nmodel: {model}\ntokens: {prompt_tokens}+{completion_tokens}\n\n{text}")
        }
        EventKind::ToolCall { name, args } => format!("TOOL CALL\n\n{name}\n\nargs: {args}"),
        EventKind::ToolResult { name, ok, output } => {
            format!("TOOL RESULT\n\n{name} (ok={ok})\n\n{output}")
        }
        EventKind::PolicyDecision {
            tool_name,
            operation,
            decision,
            reason,
        } => {
            format!(
                "POLICY DECISION\n\ntool: {tool_name}\noperation: {operation}\ndecision: {decision}\n\n{reason}"
            )
        }
        EventKind::Diff { path, patch } => format!("DIFF {path}\n\n{patch}"),
        EventKind::VerificationGate { passed, detail } => {
            format!("VERIFICATION GATE\n\npassed: {passed}\n\n{detail}")
        }
        EventKind::RouteDecision {
            from_model,
            to_model,
            reason,
        } => {
            format!("ROUTE DECISION\n\n{from_model} → {to_model}\n\n{reason}")
        }
        EventKind::Note { text } => format!("NOTE\n\n{text}"),
    }
}

/// 渲染(双栏:左详情 60% + 右时间线 40% + 底状态栏)。
pub fn ui(f: &mut Frame, app: &App) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::text::Line;
    use ratatui::widgets::{Paragraph, Wrap};

    // 垂直:主体 + 底部状态栏
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    // 水平:左详情(60%) + 右时间线(40%)
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(outer[0]);

    // 左:选中 step 详情
    let detail = app
        .selected_event()
        .map(event_detail)
        .unwrap_or_else(|| "(no event)".into());
    let detail_widget = Paragraph::new(detail)
        .block(Block::default().borders(Borders::ALL).title("Detail"))
        .wrap(Wrap { trim: false });
    f.render_widget(detail_widget, panes[0]);

    // 右:时间线 List(高亮选中)
    let items: Vec<ListItem> = app
        .events
        .iter()
        .map(|e| ListItem::new(event_summary(e)))
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.selected));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Timeline"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, panes[1], &mut state);

    // 底:状态栏
    let status = Line::from(format!(
        " step {}/{}  —  ↑/↓ or j/k select · q quit ",
        app.selected + 1,
        app.events.len()
    ));
    f.render_widget(Paragraph::new(status), outer[1]);
}

/// 打开 trace 并进入 TUI。
pub fn run_tui(path: &Path) -> Result<()> {
    let events = read_trace(path)?;
    if events.is_empty() {
        anyhow::bail!("trace {} is empty — nothing to show", path.display());
    }
    let mut app = App::new(events);

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
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn sample_events() -> Vec<TraceEvent> {
        vec![
            TraceEvent {
                step: 0,
                ts_ms: 0,
                kind: EventKind::TaskStarted {
                    task: "build X".into(),
                },
            },
            TraceEvent {
                step: 1,
                ts_ms: 0,
                kind: EventKind::Thought {
                    text: "thinking".into(),
                },
            },
            TraceEvent {
                step: 2,
                ts_ms: 0,
                kind: EventKind::Note {
                    text: "done".into(),
                },
            },
        ]
    }

    #[test]
    fn select_moves_within_bounds() {
        let mut app = App::new(sample_events());
        assert_eq!(app.selected, 0);
        app.select_prev(); // 已在顶部,不动
        assert_eq!(app.selected, 0);
        app.select_next();
        assert_eq!(app.selected, 1);
        app.select_next();
        app.select_next(); // 已在底部(共 3 个),不越界
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn ui_renders_two_panes_and_status() {
        let mut app = App::new(sample_events());
        app.select_next(); // 选中 step 1 (Thought "thinking")
        let mut terminal = Terminal::new(TestBackend::new(100, 16)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        // 右栏时间线
        assert!(
            text.contains("Timeline"),
            "should have timeline pane: {text}"
        );
        assert!(text.contains("TASK"), "timeline should list events: {text}");
        // 左栏详情(选中 Thought)
        assert!(text.contains("Detail"), "should have detail pane: {text}");
        assert!(
            text.contains("thinking"),
            "detail should show selected event content: {text}"
        );
        // 底部状态栏
        assert!(
            text.contains("step 2/3"),
            "status bar should show position: {text}"
        );
    }
}

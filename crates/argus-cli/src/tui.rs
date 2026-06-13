//! Argus TUI —— 双栏 Trace 浏览器(右时间线、左详情、底状态)。
//!
//! Elm 式拆分:App(状态)/ ui(渲染,可 TestBackend 测)/ run_tui(event loop)。

use anyhow::Result;
use argus_trace::{read_trace, EventKind, TraceEvent};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
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
        Self { events, selected: 0 }
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
    #[allow(dead_code)] // P9-T2 双栏详情面板会用到
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
        EventKind::Diff { .. } => "DIFF",
        EventKind::VerificationGate { .. } => "GATE",
        EventKind::RouteDecision { .. } => "ROUTE",
        EventKind::Note { .. } => "NOTE",
    };
    format!("[{:>3}] {}", e.step, tag)
}

/// 渲染(walking skeleton:单栏时间线 List,高亮选中项)。
pub fn ui(f: &mut Frame, app: &App) {
    let items: Vec<ListItem> = app.events.iter().map(|e| ListItem::new(event_summary(e))).collect();
    let mut state = ListState::default();
    state.select(Some(app.selected));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Argus Trace — ↑/↓ select, q quit"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, f.area(), &mut state);
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

fn event_loop(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, app: &mut App) -> Result<()> {
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
            TraceEvent { step: 0, ts_ms: 0, kind: EventKind::TaskStarted { task: "build X".into() } },
            TraceEvent { step: 1, ts_ms: 0, kind: EventKind::Thought { text: "thinking".into() } },
            TraceEvent { step: 2, ts_ms: 0, kind: EventKind::Note { text: "done".into() } },
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
    fn ui_renders_timeline() {
        let app = App::new(sample_events());
        let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();
        terminal.draw(|f| ui(f, &app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("TASK"), "timeline should show event tags: {text}");
        assert!(text.contains("Argus Trace"), "should show title: {text}");
    }
}

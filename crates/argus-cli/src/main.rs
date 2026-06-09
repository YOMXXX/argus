use anyhow::Result;
use argus_core::{
    task_from_trace, Agent, AnthropicProvider, AutoApprover, MockProvider, ReadFile, RunShell,
    WriteFile,
};
use argus_core::Approver;
use argus_trace::{read_trace, EventKind, TraceWriter};
use clap::{Parser, Subcommand};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Argus — the AI coding agent that never blinks.
#[derive(Parser)]
#[command(
    name = "argus",
    version,
    about = "Argus never blinks. The production-reliable, model-agnostic AI coding agent."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a task through the agent.
    Run {
        /// The task for the agent.
        task: String,
        /// Model name recorded in the trace.
        #[arg(long, default_value = "mock")]
        model: String,
        /// Where to write the trace (JSONL).
        #[arg(long, default_value = ".argus/trace.jsonl")]
        trace: PathBuf,
        /// Provider to use. 'mock' needs no API key; 'anthropic' reads ANTHROPIC_API_KEY (pass --model claude-*).
        #[arg(long, default_value = "mock")]
        provider: String,
        /// Auto-approve all tool calls (skip the approval prompt).
        #[arg(long)]
        yes: bool,
    },
    /// Inspect a recorded trace.
    Trace {
        #[command(subcommand)]
        command: TraceCommands,
    },
}

#[derive(Subcommand)]
enum TraceCommands {
    /// Show the timeline of a trace file.
    Show {
        /// Path to the trace JSONL file.
        #[arg(default_value = ".argus/trace.jsonl")]
        path: PathBuf,
    },
    /// Re-run a recorded trace's task with a different provider/model (time travel).
    Fork {
        /// Source trace to fork from.
        path: PathBuf,
        #[arg(long, default_value = "mock")]
        provider: String,
        #[arg(long, default_value = "mock")]
        model: String,
        /// Where to write the forked trace.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Compare two traces step by step (time-travel diff).
    Diff {
        /// First trace.
        a: PathBuf,
        /// Second trace.
        b: PathBuf,
    },
}

/// 终端审批：打印命令、读 stdin 一行 y/n；非交互/EOF 视为拒绝。
struct StdinApprover;
impl Approver for StdinApprover {
    fn approve(&self, tool_name: &str, args: &str) -> bool {
        eprint!("[approval] {tool_name} {args}\n  allow? [y/N] ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => false,
            Ok(_) => matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Run { task, model, trace, provider, yes } => {
            run(&provider, &task, &model, &trace, yes).await
        }
        Commands::Trace { command } => match command {
            TraceCommands::Show { path } => trace_show(&path),
            TraceCommands::Fork { path, provider, model, out } => {
                trace_fork(&path, &provider, &model, out.as_deref()).await
            }
            TraceCommands::Diff { a, b } => trace_diff(&a, &b),
        },
    }
}

async fn run_agent(provider: &str, model: &str, task: &str, trace_path: &Path, yes: bool) -> Result<String> {
    if let Some(parent) = trace_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut trace = TraceWriter::create(trace_path)?;
    let make_approver = || -> Box<dyn Approver> {
        if yes { Box::new(AutoApprover) } else { Box::new(StdinApprover) }
    };
    let output = match provider {
        "mock" => {
            let p = MockProvider::new();
            Agent::new(&p, model, &mut trace)
                .with_tools(vec![
                    Box::new(ReadFile::new(".")),
                    Box::new(WriteFile::new(".")),
                    Box::new(RunShell::new(".")),
                ])
                .with_approver(make_approver())
                .run(task)
                .await?
        }
        "anthropic" => {
            if model == "mock" {
                eprintln!("warning: --model is 'mock'; for Anthropic pass e.g. --model claude-sonnet-4-5");
            }
            let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                anyhow::anyhow!("ANTHROPIC_API_KEY not set (required for --provider anthropic)")
            })?;
            let p = AnthropicProvider::new(key);
            Agent::new(&p, model, &mut trace)
                .with_tools(vec![
                    Box::new(ReadFile::new(".")),
                    Box::new(WriteFile::new(".")),
                    Box::new(RunShell::new(".")),
                ])
                .with_approver(make_approver())
                .run(task)
                .await?
        }
        other => anyhow::bail!("unknown provider '{other}' (expected: mock | anthropic)"),
    };
    Ok(output)
}

async fn run(provider: &str, task: &str, model: &str, trace_path: &Path, yes: bool) -> Result<()> {
    let output = run_agent(provider, model, task, trace_path, yes).await?;
    println!("{output}");
    eprintln!("(trace written to {})", trace_path.display());
    Ok(())
}

async fn trace_fork(src: &Path, provider: &str, model: &str, out: Option<&Path>) -> Result<()> {
    let events = read_trace(src)?;
    let task = task_from_trace(&events).ok_or_else(|| {
        anyhow::anyhow!(
            "trace {} has no TaskStarted event; re-run the task to enable fork",
            src.display()
        )
    })?;
    let out_path = out
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| src.with_extension("fork.jsonl"));
    eprintln!("forking task from {}: {task:?}", src.display());
    let output = run_agent(provider, model, &task, &out_path, true).await?;
    println!("{output}");
    eprintln!("(forked trace written to {})", out_path.display());
    Ok(())
}

fn summarize(kind: &EventKind) -> String {
    match kind {
        EventKind::TaskStarted { task } => format!("TASK     {task}"),
        EventKind::Thought { text } => format!("THOUGHT  {text}"),
        EventKind::ModelRequest { model, prompt_tokens } => {
            format!("MODEL ->  {model} ({prompt_tokens} prompt tokens)")
        }
        EventKind::ModelResponse { model, prompt_tokens, completion_tokens, text } => {
            format!("MODEL <-  {model} ({prompt_tokens}+{completion_tokens} tokens): {text}")
        }
        EventKind::ToolCall { name, args } => format!("TOOL ->   {name}({args})"),
        EventKind::ToolResult { name, ok, output } => {
            format!("TOOL <-   {name} ok={ok}: {output}")
        }
        EventKind::Diff { path, .. } => format!("DIFF     {path}"),
        EventKind::VerificationGate { passed, detail } => {
            format!("GATE     passed={passed}: {detail}")
        }
        EventKind::Note { text } => format!("NOTE     {text}"),
    }
}

fn trace_show(path: &Path) -> Result<()> {
    let events = read_trace(path)?;
    if events.is_empty() {
        println!("(empty trace)");
        return Ok(());
    }
    for e in &events {
        println!("[{:>4}] {}", e.step, summarize(&e.kind));
    }
    Ok(())
}

fn trace_diff(a_path: &Path, b_path: &Path) -> Result<()> {
    let a = read_trace(a_path)?;
    let b = read_trace(b_path)?;
    let n = a.len().max(b.len());
    println!("step | A: {} | B: {}", a_path.display(), b_path.display());
    for i in 0..n {
        let la = a.get(i).map(|e| summarize(&e.kind)).unwrap_or_else(|| "—".into());
        let lb = b.get(i).map(|e| summarize(&e.kind)).unwrap_or_else(|| "—".into());
        let mark = if la == lb { " " } else { "≠" };
        println!("[{i:>3}] {mark} A: {la}");
        println!("      {mark} B: {lb}");
    }
    Ok(())
}

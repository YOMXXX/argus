use anyhow::Result;
use argus_core::{Agent, MockProvider};
use argus_trace::{read_trace, EventKind, TraceWriter};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
    /// Run a task through the agent (built-in mock provider for now).
    Run {
        /// The task for the agent.
        task: String,
        /// Model name recorded in the trace.
        #[arg(long, default_value = "mock")]
        model: String,
        /// Where to write the trace (JSONL).
        #[arg(long, default_value = ".argus/trace.jsonl")]
        trace: PathBuf,
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Run { task, model, trace } => run(&task, &model, &trace),
        Commands::Trace { command } => match command {
            TraceCommands::Show { path } => trace_show(&path),
        },
    }
}

fn run(task: &str, model: &str, trace_path: &PathBuf) -> Result<()> {
    if let Some(parent) = trace_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let provider = MockProvider::new();
    let mut trace = TraceWriter::create(trace_path)?;
    let mut agent = Agent::new(&provider, model, &mut trace);
    let output = agent.run(task)?;
    println!("{output}");
    eprintln!("(trace written to {})", trace_path.display());
    Ok(())
}

fn trace_show(path: &PathBuf) -> Result<()> {
    let events = read_trace(path)?;
    if events.is_empty() {
        println!("(empty trace)");
        return Ok(());
    }
    for e in &events {
        let summary = match &e.kind {
            EventKind::Thought { text } => format!("THOUGHT  {text}"),
            EventKind::ModelRequest { model, prompt_tokens } => {
                format!("MODEL ->  {model} ({prompt_tokens} prompt tokens)")
            }
            EventKind::ModelResponse { model, completion_tokens, text } => {
                format!("MODEL <-  {model} ({completion_tokens} tokens): {text}")
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
        };
        println!("[{:>4}] {summary}", e.step);
    }
    Ok(())
}

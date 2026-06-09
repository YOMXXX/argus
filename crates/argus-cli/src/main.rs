use anyhow::Result;
use argus_core::{Agent, AnthropicProvider, MockProvider};
use argus_trace::{read_trace, EventKind, TraceWriter};
use clap::{Parser, Subcommand};
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

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Run { task, model, trace, provider } => run(&provider, &task, &model, &trace).await,
        Commands::Trace { command } => match command {
            TraceCommands::Show { path } => trace_show(&path),
        },
    }
}

async fn run(provider: &str, task: &str, model: &str, trace_path: &Path) -> Result<()> {
    if let Some(parent) = trace_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut trace = TraceWriter::create(trace_path)?;

    let output = match provider {
        "mock" => {
            let p = MockProvider::new();
            Agent::new(&p, model, &mut trace).run(task).await?
        }
        "anthropic" => {
            if model == "mock" {
                eprintln!("warning: --model is 'mock'; for Anthropic pass e.g. --model claude-sonnet-4-5");
            }
            let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                anyhow::anyhow!("ANTHROPIC_API_KEY not set (required for --provider anthropic)")
            })?;
            let p = AnthropicProvider::new(key);
            Agent::new(&p, model, &mut trace).run(task).await?
        }
        other => anyhow::bail!("unknown provider '{other}' (expected: mock | anthropic)"),
    };

    println!("{output}");
    eprintln!("(trace written to {})", trace_path.display());
    Ok(())
}

fn trace_show(path: &Path) -> Result<()> {
    let events = read_trace(path)?;
    if events.is_empty() {
        println!("(empty trace)");
        return Ok(());
    }
    for e in &events {
        let summary = match &e.kind {
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
        };
        println!("[{:>4}] {summary}", e.step);
    }
    Ok(())
}

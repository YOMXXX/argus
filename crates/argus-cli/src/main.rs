mod tui;

use anyhow::Result;
use argus_core::{
    mcp_connect, run_suite, run_with_escalation, Approver, CommandVerifier, EvalSuite, Verifier,
};
use argus_core::{
    task_from_trace, Agent, AnthropicProvider, AutoApprover, MockProvider, OpenAiProvider,
    Provider, ReadFile, RunShell, WriteFile,
};
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
        /// Verification command(s) that must pass before the agent is "done" (repeatable).
        #[arg(long = "verify")]
        verify: Vec<String>,
        /// Override the provider base URL (OpenAI-compatible endpoints: OpenRouter, local Ollama, …).
        #[arg(long = "base-url")]
        base_url: Option<String>,
        /// Rules file to load as system prompt (default: auto-discover AGENTS.md / CLAUDE.md).
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Disable auto-discovery of AGENTS.md / CLAUDE.md.
        #[arg(long = "no-rules")]
        no_rules: bool,
        /// Connect an MCP server (command line) and inject its tools, e.g. --mcp "npx -y @modelcontextprotocol/server-everything".
        #[arg(long)]
        mcp: Option<String>,
    },
    /// Inspect a recorded trace.
    Trace {
        #[command(subcommand)]
        command: TraceCommands,
    },
    /// Run an eval suite: batch-run cases and report pass-rate.
    Eval {
        /// Path to the suite JSON file.
        suite: PathBuf,
        /// Provider to use ('mock' needs no key; 'anthropic' reads ANTHROPIC_API_KEY).
        #[arg(long, default_value = "mock")]
        provider: String,
        /// Model name (pass claude-* for --provider anthropic).
        #[arg(long, default_value = "mock")]
        model: String,
        /// Directory for per-case traces.
        #[arg(long = "out-dir", default_value = ".argus/eval")]
        out_dir: PathBuf,
        /// Override the provider base URL (OpenAI-compatible endpoints: OpenRouter, local Ollama, …).
        #[arg(long = "base-url")]
        base_url: Option<String>,
    },
    /// Cost-smart routing: cheap model first, escalate to strong on verify failure.
    Route {
        /// The task for the agent.
        task: String,
        /// Cheap model to try first.
        #[arg(long)]
        cheap: String,
        /// Strong model to escalate to on verification failure.
        #[arg(long)]
        strong: String,
        /// Verification command(s) that decide pass/escalate (repeatable, required).
        #[arg(long = "verify")]
        verify: Vec<String>,
        /// Provider to use ('mock' needs no key; 'anthropic' reads ANTHROPIC_API_KEY).
        #[arg(long, default_value = "mock")]
        provider: String,
        /// Where to write the trace (JSONL).
        #[arg(long, default_value = ".argus/route.jsonl")]
        trace: PathBuf,
        /// Override the provider base URL (OpenAI-compatible endpoints: OpenRouter, local Ollama, …).
        #[arg(long = "base-url")]
        base_url: Option<String>,
    },
    /// (internal) Minimal MCP server over stdio for end-to-end tests.
    #[command(name = "__mcp-mock", hide = true)]
    McpMock,
    /// Run Argus as an MCP server: expose its reliability tools (e.g. `verify`) to any MCP client (Claude Code, Cursor, …).
    #[command(name = "mcp-serve")]
    McpServe,
    /// Open a trace in an interactive TUI (two-pane timeline browser).
    Tui {
        /// Path to the trace JSONL file.
        #[arg(default_value = ".argus/trace.jsonl")]
        path: PathBuf,
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

/// 自动发现工作目录的规则文件(AGENTS.md 优先,其次 CLAUDE.md),返回 (文件名, 内容)。
fn discover_rules(dir: &Path) -> Option<(String, String)> {
    for name in ["AGENTS.md", "CLAUDE.md"] {
        let p = dir.join(name);
        if let Ok(content) = std::fs::read_to_string(&p) {
            if !content.trim().is_empty() {
                return Some((name.to_string(), content));
            }
        }
    }
    None
}

/// 解析最终的 system prompt:--no-rules 关闭;--rules <file> 显式;否则自动发现。
/// 加载成功时往 stderr 打印来源(便于用户确认)。
fn resolve_rules(rules: Option<&Path>, no_rules: bool) -> Result<Option<String>> {
    if no_rules {
        return Ok(None);
    }
    if let Some(file) = rules {
        let content = std::fs::read_to_string(file)
            .map_err(|e| anyhow::anyhow!("failed to read --rules {}: {e}", file.display()))?;
        eprintln!("(loaded rules from {})", file.display());
        return Ok(Some(content));
    }
    match discover_rules(Path::new(".")) {
        Some((name, content)) => {
            eprintln!("(loaded rules from {name})");
            Ok(Some(content))
        }
        None => Ok(None),
    }
}

/// 按名字构造 provider;`base_url` 可覆盖端点(OpenAI 兼容端点如 OpenRouter / 本地 Ollama)。
fn make_provider(provider: &str, base_url: Option<&str>) -> Result<Box<dyn Provider>> {
    match provider {
        "mock" => Ok(Box::new(MockProvider::new())),
        "anthropic" => {
            let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                anyhow::anyhow!("ANTHROPIC_API_KEY not set (required for --provider anthropic)")
            })?;
            Ok(match base_url {
                Some(u) => Box::new(AnthropicProvider::with_base_url(key, u)),
                None => Box::new(AnthropicProvider::new(key)),
            })
        }
        "openai" => {
            let key = std::env::var("OPENAI_API_KEY").map_err(|_| {
                anyhow::anyhow!("OPENAI_API_KEY not set (required for --provider openai)")
            })?;
            Ok(match base_url {
                Some(u) => Box::new(OpenAiProvider::with_base_url(key, u)),
                None => Box::new(OpenAiProvider::new(key)),
            })
        }
        other => anyhow::bail!("unknown provider '{other}' (expected: mock | anthropic | openai)"),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Run {
            task,
            model,
            trace,
            provider,
            yes,
            verify,
            base_url,
            rules,
            no_rules,
            mcp,
        } => {
            run(
                &provider,
                &task,
                &model,
                &trace,
                yes,
                &verify,
                base_url.as_deref(),
                rules.as_deref(),
                no_rules,
                mcp.as_deref(),
            )
            .await
        }
        Commands::Trace { command } => match command {
            TraceCommands::Show { path } => trace_show(&path),
            TraceCommands::Fork {
                path,
                provider,
                model,
                out,
            } => trace_fork(&path, &provider, &model, out.as_deref()).await,
            TraceCommands::Diff { a, b } => trace_diff(&a, &b),
        },
        Commands::Eval {
            suite,
            provider,
            model,
            out_dir,
            base_url,
        } => eval_run(&suite, &provider, &model, &out_dir, base_url.as_deref()).await,
        Commands::Route {
            task,
            cheap,
            strong,
            verify,
            provider,
            trace,
            base_url,
        } => {
            route_run(
                &task,
                &cheap,
                &strong,
                &verify,
                &provider,
                &trace,
                base_url.as_deref(),
            )
            .await
        }
        Commands::McpMock => mcp_mock().await,
        Commands::McpServe => mcp_serve().await,
        Commands::Tui { path } => tui::run_tui(&path),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_agent(
    provider: &str,
    model: &str,
    task: &str,
    trace_path: &Path,
    yes: bool,
    verify: &[String],
    base_url: Option<&str>,
    system: Option<String>,
    mcp: Option<&str>,
) -> Result<String> {
    if let Some(parent) = trace_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut trace = TraceWriter::create(trace_path)?;
    let p = make_provider(provider, base_url)?;
    if provider != "mock" && model == "mock" {
        eprintln!("warning: --model is 'mock'; pass a real model (e.g. --model claude-sonnet-4-5 or gpt-4o-mini)");
    }
    let make_approver = || -> Box<dyn Approver> {
        if yes {
            Box::new(AutoApprover)
        } else {
            Box::new(StdinApprover)
        }
    };
    let make_verifier = || -> Option<Box<dyn Verifier>> {
        if verify.is_empty() {
            None
        } else {
            Some(Box::new(CommandVerifier::new(".", verify.to_vec())))
        }
    };
    let mut tools: Vec<Box<dyn argus_core::Tool>> = Vec::new();
    if let Some(cmd) = mcp {
        let mut parts = cmd.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("--mcp needs a command"))?;
        let mcp_args: Vec<String> = parts.map(|s| s.to_string()).collect();
        let mcp_tools = mcp_connect(program, &mcp_args)
            .await
            .map_err(|e| anyhow::anyhow!("failed to connect MCP server: {e}"))?;
        eprintln!("(connected MCP server: {} tool(s))", mcp_tools.len());
        tools.extend(mcp_tools);
    }
    tools.push(Box::new(ReadFile::new(".")));
    tools.push(Box::new(WriteFile::new(".")));
    tools.push(Box::new(RunShell::new(".")));
    let mut agent = Agent::new(&*p, model, &mut trace)
        .with_tools(tools)
        .with_approver(make_approver());
    if let Some(s) = system {
        agent = agent.with_system(s);
    }
    if let Some(v) = make_verifier() {
        agent = agent.with_verifier(v);
    }
    agent.run(task).await
}

#[allow(clippy::too_many_arguments)]
async fn run(
    provider: &str,
    task: &str,
    model: &str,
    trace_path: &Path,
    yes: bool,
    verify: &[String],
    base_url: Option<&str>,
    rules: Option<&Path>,
    no_rules: bool,
    mcp: Option<&str>,
) -> Result<()> {
    let system = resolve_rules(rules, no_rules)?;
    let output = run_agent(
        provider, model, task, trace_path, yes, verify, base_url, system, mcp,
    )
    .await?;
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
    let output = run_agent(
        provider,
        model,
        &task,
        &out_path,
        true,
        &[],
        None,
        None,
        None,
    )
    .await?;
    println!("{output}");
    eprintln!("(forked trace written to {})", out_path.display());
    Ok(())
}

fn summarize(kind: &EventKind) -> String {
    match kind {
        EventKind::TaskStarted { task } => format!("TASK     {task}"),
        EventKind::Thought { text } => format!("THOUGHT  {text}"),
        EventKind::ModelRequest {
            model,
            prompt_tokens,
        } => {
            format!("MODEL ->  {model} ({prompt_tokens} prompt tokens)")
        }
        EventKind::ModelResponse {
            model,
            prompt_tokens,
            completion_tokens,
            text,
        } => {
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
        EventKind::RouteDecision {
            from_model,
            to_model,
            reason,
        } => {
            format!("ROUTE    {from_model} → {to_model}: {reason}")
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
        let la = a
            .get(i)
            .map(|e| summarize(&e.kind))
            .unwrap_or_else(|| "—".into());
        let lb = b
            .get(i)
            .map(|e| summarize(&e.kind))
            .unwrap_or_else(|| "—".into());
        let mark = if la == lb { " " } else { "≠" };
        println!("[{i:>3}] {mark} A: {la}");
        println!("      {mark} B: {lb}");
    }
    Ok(())
}

async fn eval_run(
    suite_path: &Path,
    provider: &str,
    model: &str,
    out_dir: &Path,
    base_url: Option<&str>,
) -> Result<()> {
    let text = std::fs::read_to_string(suite_path)
        .map_err(|e| anyhow::anyhow!("failed to read suite {}: {e}", suite_path.display()))?;
    let suite: EvalSuite = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("invalid suite JSON {}: {e}", suite_path.display()))?;
    // base_dir = suite 文件所在目录(解析 case 相对 dir);无父目录时用 "."
    let base_dir = suite_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let p = make_provider(provider, base_url)?;
    let report = run_suite(&suite, base_dir, &*p, model, out_dir).await?;

    println!("eval: {} ({} case(s))", report.suite_name, report.total());
    for r in &report.results {
        let tag = if r.passed { "PASS" } else { "FAIL" };
        println!("[{tag}] {}  → {}", r.id, r.trace_path.display());
    }
    let pct = (report.pass_rate() * 100.0).round() as u64;
    println!(
        "{}/{} passed ({}%)",
        report.passed_count(),
        report.total(),
        pct
    );

    // CI gate:有 case 失败 → 退出码非 0(trace 已在 run_suite 内写完并 flush)。
    if !report.all_passed() {
        std::process::exit(1);
    }
    Ok(())
}

async fn route_run(
    task: &str,
    cheap: &str,
    strong: &str,
    verify: &[String],
    provider: &str,
    trace_path: &Path,
    base_url: Option<&str>,
) -> Result<()> {
    if verify.is_empty() {
        anyhow::bail!("--verify is required for route (it decides whether to escalate)");
    }
    let work_dir = Path::new(".");

    let p = make_provider(provider, base_url)?;
    let report =
        run_with_escalation(&*p, cheap, strong, work_dir, trace_path, verify, task).await?;

    println!("{}", report.final_text);
    let status = if report.passed { "passed" } else { "failed" };
    if report.escalated {
        println!(
            "route: escalated {} → {} ({status})",
            report.cheap_model, report.strong_model
        );
    } else {
        println!("route: stayed on {} ({status})", report.cheap_model);
    }
    let actual = report.actual_cost();
    let saved = report.always_strong_cost - actual;
    println!(
        "cost: ${:.4} actual (cheap ${:.4} + strong ${:.4}); vs always-strong ${:.4} → saved ${:.4}",
        actual, report.cheap_cost, report.strong_cost, report.always_strong_cost, saved
    );
    eprintln!("(trace written to {})", trace_path.display());
    Ok(())
}

/// Argus 作为 MCP server(stdio,newline JSON-RPC 2.0):把可靠性能力暴露成工具。
async fn mcp_serve() -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let msg: serde_json::Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = msg.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let response = match method {
            "initialize" => Some(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "argus", "version": env!("CARGO_PKG_VERSION")}
                }
            })),
            "tools/list" => Some(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "result": {"tools": [{
                    "name": "verify",
                    "description": "Run verification commands (build/test/lint) in a directory; all must exit 0 to pass. Use this to prove a task is actually done before claiming success.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "commands": {"type": "array", "items": {"type": "string"}, "description": "Shell commands that must all exit 0."},
                            "dir": {"type": "string", "description": "Working directory (default: current)."}
                        },
                        "required": ["commands"]
                    }
                }]}
            })),
            "tools/call" => {
                let params = msg
                    .get("params")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let call = mcp_serve_tool(name, &args).await;
                Some(match call {
                    Ok(text) => serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {"content": [{"type": "text", "text": text}]}
                    }),
                    Err(e) => serde_json::json!({
                        "jsonrpc": "2.0", "id": id,
                        "result": {"content": [{"type": "text", "text": format!("error: {e}")}], "isError": true}
                    }),
                })
            }
            _ => None, // 通知(如 notifications/initialized)无需响应
        };
        if let Some(resp) = response {
            stdout.write_all(format!("{resp}\n").as_bytes()).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

/// 执行 mcp-serve 暴露的工具。
async fn mcp_serve_tool(name: &str, args: &serde_json::Value) -> Result<String> {
    match name {
        "verify" => {
            let commands: Vec<String> = args
                .get("commands")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if commands.is_empty() {
                anyhow::bail!("verify: 'commands' is required and must be non-empty");
            }
            let dir = args.get("dir").and_then(|v| v.as_str()).unwrap_or(".");
            let result = CommandVerifier::new(dir, commands).verify().await;
            Ok(format!("passed: {}\n{}", result.passed, result.detail))
        }
        other => anyhow::bail!("unknown tool '{other}'"),
    }
}

/// 一个最小 MCP server(stdio,newline JSON-RPC):提供单个 `echo` 工具。仅用于端到端测试。
async fn mcp_mock() -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let msg: serde_json::Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = msg.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let response = match method {
            "initialize" => Some(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "result": {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "argus-mock", "version": "1"}}
            })),
            "tools/list" => Some(serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "result": {"tools": [{
                    "name": "echo",
                    "description": "Echo back the msg argument.",
                    "inputSchema": {"type": "object", "properties": {"msg": {"type": "string"}}, "required": ["msg"]}
                }]}
            })),
            "tools/call" => {
                let m = msg
                    .get("params")
                    .and_then(|p| p.get("arguments"))
                    .and_then(|a| a.get("msg"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                Some(serde_json::json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": {"content": [{"type": "text", "text": format!("echo: {m}")}]}
                }))
            }
            _ => None, // 通知(如 notifications/initialized)无需响应
        };
        if let Some(resp) = response {
            stdout.write_all(format!("{resp}\n").as_bytes()).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

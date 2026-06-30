use anyhow::Result;
use argus_cli::tui;
use argus_core::{
    mcp_connect, run_suite_with_options, run_with_escalation, Approver, CommandVerifier,
    EvalIsolation, EvalRunOptions, EvalSuite, SuiteReport, Verifier,
};
use argus_core::{
    task_from_trace_at, Agent, AnthropicProvider, AutoApprover, CompletionRequest,
    CompletionResponse, ListFiles, MockProvider, OpenAiProvider, OperationKind, Provider, ReadFile,
    RunShell, SandboxPolicy, SearchText, StopReason, ToolCall, Usage, WriteFile,
};
use argus_trace::{read_trace, EventKind, TraceWriter};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::BTreeSet;
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

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SandboxArg {
    WorkspaceWrite,
    ReadOnly,
    Trusted,
}

impl SandboxArg {
    fn policy(self) -> SandboxPolicy {
        match self {
            Self::WorkspaceWrite => SandboxPolicy::workspace_write(),
            Self::ReadOnly => SandboxPolicy::read_only(),
            Self::Trusted => SandboxPolicy::trusted(),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::WorkspaceWrite => "workspace-write",
            Self::ReadOnly => "read-only",
            Self::Trusted => "trusted",
        }
    }
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
        /// Allow an external MCP tool to be injected. Repeatable; required when --yes is used with --mcp.
        #[arg(long = "mcp-allow")]
        mcp_allow: Vec<String>,
        /// Sandbox policy for tool execution.
        #[arg(long, value_enum, default_value_t = SandboxArg::WorkspaceWrite)]
        sandbox: SandboxArg,
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
        /// Number of repeated samples per case.
        #[arg(long, default_value_t = 1)]
        samples: usize,
        /// Disable the agent self-repair gate; final verify still decides pass/fail.
        #[arg(long = "no-gate")]
        no_gate: bool,
        /// Write a machine-readable JSON report.
        #[arg(long = "report-json")]
        report_json: Option<PathBuf>,
        /// Run eval cases directly in their original directories. Default uses isolated temp workspaces.
        #[arg(long = "in-place")]
        in_place: bool,
        /// Accepted for compatibility; eval case tools are auto-approved.
        #[arg(long, hide = true)]
        yes: bool,
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
    /// Check local Argus installation and provider environment.
    Doctor,
    /// Run a zero-config verification-gate demo and write a replayable trace.
    Demo {
        /// Workspace directory for demo files. Defaults to a temporary directory.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Inspect Argus security policy behavior.
    Policy {
        #[command(subcommand)]
        command: PolicyCommands,
    },
    /// (internal) Minimal MCP server over stdio for end-to-end tests.
    #[command(name = "__mcp-mock", hide = true)]
    McpMock,
    /// Run Argus as an MCP server: expose its reliability tools (e.g. `verify`) to any MCP client (Claude Code, Cursor, …).
    #[command(name = "mcp-serve")]
    McpServe {
        /// Workspace root that all MCP tool paths must stay within.
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// Expose an additional MCP server tool. Repeatable; default exposes only `verify`.
        #[arg(long = "allow-tool")]
        allow_tool: Vec<String>,
    },
    /// Open a trace in an interactive TUI (two-pane timeline browser).
    Tui {
        /// Path to the trace JSONL file.
        #[arg(default_value = ".argus/trace.jsonl")]
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum PolicyCommands {
    /// Show allow/ask/deny decisions for built-in operation classes.
    Show {
        /// Sandbox policy to explain.
        #[arg(long, value_enum, default_value_t = SandboxArg::WorkspaceWrite)]
        sandbox: SandboxArg,
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
        /// Fork from a specific trace step by injecting prior context into the new task.
        #[arg(long)]
        step: Option<u64>,
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

fn doctor() -> Result<()> {
    println!("Argus doctor");
    println!("binary: argus {}", env!("CARGO_PKG_VERSION"));
    println!("cwd: {}", std::env::current_dir()?.display());
    println!(
        "git: {}",
        if command_available("git") {
            "available"
        } else {
            "missing"
        }
    );
    println!("providers:");
    println!("  mock: available (no API key required)");
    println!(
        "  anthropic: {}",
        if std::env::var_os("ANTHROPIC_API_KEY").is_some() {
            "configured"
        } else {
            "missing ANTHROPIC_API_KEY"
        }
    );
    println!(
        "  openai: {}",
        if std::env::var_os("OPENAI_API_KEY").is_some() {
            "configured"
        } else {
            "missing OPENAI_API_KEY"
        }
    );
    println!("mcp: use `argus mcp-serve --workspace <repo>`; default tool exposure is verify only");
    Ok(())
}

fn command_available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn policy_show(sandbox: SandboxArg) -> Result<()> {
    let policy = sandbox.policy();
    println!("sandbox: {}", sandbox.name());
    println!("tool\toperation\tdecision\treason");
    for (tool_name, operation) in [
        ("read_file", OperationKind::Read),
        ("write_file", OperationKind::Write),
        ("run_shell", OperationKind::Shell),
        ("external_mcp", OperationKind::Mcp),
    ] {
        let decision = policy.decide(operation, tool_name);
        println!(
            "{}\t{}\t{}\t{}",
            tool_name,
            operation.as_str(),
            decision.action.as_str(),
            decision.reason
        );
    }
    println!("note: this is a workspace/policy boundary, not an OS-level container sandbox.");
    Ok(())
}

struct DemoProvider;

#[async_trait::async_trait]
impl Provider for DemoProvider {
    fn name(&self) -> &str {
        "demo"
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse> {
        let has_tool_result = req.messages.iter().any(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, argus_core::Content::ToolResult { .. }))
        });
        let saw_verify_failure = req
            .messages
            .iter()
            .any(|m| m.text().contains("Verification failed"));
        let usage = Usage {
            prompt_tokens: req
                .messages
                .iter()
                .map(|m| m.text().split_whitespace().count() as u64)
                .sum(),
            completion_tokens: 8,
        };
        if saw_verify_failure && !has_tool_result {
            return Ok(CompletionResponse {
                text: "The gate caught the fake completion. Writing the fix now.".into(),
                tool_calls: vec![ToolCall {
                    id: "demo-fix".into(),
                    name: "write_file".into(),
                    input: serde_json::json!({"path": "answer.txt", "content": "pass"}),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
            });
        }
        if has_tool_result {
            return Ok(CompletionResponse {
                text: "Fixed after verification feedback.".into(),
                tool_calls: vec![],
                usage,
                stop_reason: StopReason::EndTurn,
            });
        }
        Ok(CompletionResponse {
            text: "Done.".into(),
            tool_calls: vec![],
            usage,
            stop_reason: StopReason::EndTurn,
        })
    }
}

async fn demo(workspace: Option<&Path>) -> Result<()> {
    let workspace = workspace
        .map(Path::to_path_buf)
        .unwrap_or_else(unique_demo_dir);
    std::fs::create_dir_all(&workspace)?;
    std::fs::write(workspace.join("answer.txt"), "fail")?;
    let trace_path = workspace.join(".argus/demo.jsonl");
    if let Some(parent) = trace_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&trace_path);

    let provider = DemoProvider;
    let mut trace = TraceWriter::create(&trace_path)?;
    let mut agent = Agent::new(&provider, "demo", &mut trace)
        .with_tools(vec![Box::new(WriteFile::new(&workspace))])
        .with_approver(Box::new(AutoApprover))
        .with_policy(SandboxPolicy::trusted())
        .with_verifier(Box::new(CommandVerifier::new(
            &workspace,
            vec!["test \"$(cat answer.txt)\" = pass".into()],
        )));
    let output = agent.run("Make answer.txt contain exactly 'pass'.").await?;
    drop(agent);

    let final_verdict =
        CommandVerifier::new(&workspace, vec!["test \"$(cat answer.txt)\" = pass".into()])
            .verify()
            .await;
    if !final_verdict.passed {
        anyhow::bail!("demo verification failed: {}", final_verdict.detail);
    }

    println!("Argus demo");
    println!("workspace: {}", workspace.display());
    println!("trace: {}", trace_path.display());
    println!("result: verification passed");
    println!("{output}");
    Ok(())
}

fn unique_demo_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("argus-demo-{}-{nanos}", std::process::id()))
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
            mcp_allow,
            sandbox,
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
                &mcp_allow,
                sandbox.policy(),
            )
            .await
        }
        Commands::Trace { command } => match command {
            TraceCommands::Show { path } => trace_show(&path),
            TraceCommands::Fork {
                path,
                step,
                provider,
                model,
                out,
            } => trace_fork(&path, step, &provider, &model, out.as_deref()).await,
            TraceCommands::Diff { a, b } => trace_diff(&a, &b),
        },
        Commands::Eval {
            suite,
            provider,
            model,
            out_dir,
            base_url,
            samples,
            no_gate,
            report_json,
            in_place,
            yes: _,
        } => {
            eval_run(
                &suite,
                &provider,
                &model,
                &out_dir,
                base_url.as_deref(),
                EvalRunOptions {
                    samples,
                    gate_enabled: !no_gate,
                    isolation: if in_place {
                        EvalIsolation::InPlace
                    } else {
                        EvalIsolation::Isolated
                    },
                },
                report_json.as_deref(),
            )
            .await
        }
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
        Commands::Doctor => doctor(),
        Commands::Demo { workspace } => demo(workspace.as_deref()).await,
        Commands::Policy { command } => match command {
            PolicyCommands::Show { sandbox } => policy_show(sandbox),
        },
        Commands::McpMock => mcp_mock().await,
        Commands::McpServe {
            workspace,
            allow_tool,
        } => mcp_serve(McpServeOptions::new(&workspace, &allow_tool)?).await,
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
    policy: SandboxPolicy,
    mcp: Option<&str>,
    mcp_allow: &[String],
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
    if mcp.is_none() && !mcp_allow.is_empty() {
        anyhow::bail!("--mcp-allow requires --mcp");
    }
    if yes && mcp.is_some() && mcp_allow.is_empty() {
        anyhow::bail!("--yes with --mcp requires at least one --mcp-allow <tool>");
    }
    if let Some(cmd) = mcp {
        let mut parts = cmd.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("--mcp needs a command"))?;
        let mcp_args: Vec<String> = parts.map(|s| s.to_string()).collect();
        let allowed_tools = (!mcp_allow.is_empty()).then_some(mcp_allow);
        let mcp_tools = mcp_connect(program, &mcp_args, allowed_tools)
            .await
            .map_err(|e| anyhow::anyhow!("failed to connect MCP server: {e}"))?;
        eprintln!("(connected MCP server: {} tool(s))", mcp_tools.len());
        tools.extend(mcp_tools);
    }
    tools.push(Box::new(ReadFile::new(".")));
    tools.push(Box::new(WriteFile::new(".")));
    tools.push(Box::new(ListFiles::new(".")));
    tools.push(Box::new(SearchText::new(".")));
    tools.push(Box::new(RunShell::new(".")));
    let mut agent = Agent::new(&*p, model, &mut trace)
        .with_tools(tools)
        .with_approver(make_approver())
        .with_policy(policy);
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
    mcp_allow: &[String],
    policy: SandboxPolicy,
) -> Result<()> {
    let system = resolve_rules(rules, no_rules)?;
    let output = run_agent(
        provider, model, task, trace_path, yes, verify, base_url, system, policy, mcp, mcp_allow,
    )
    .await?;
    println!("{output}");
    eprintln!("(trace written to {})", trace_path.display());
    Ok(())
}

async fn trace_fork(
    src: &Path,
    step: Option<u64>,
    provider: &str,
    model: &str,
    out: Option<&Path>,
) -> Result<()> {
    let events = read_trace(src)?;
    let task = task_from_trace_at(&events, step)?;
    let out_path = out
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| src.with_extension("fork.jsonl"));
    match step {
        Some(step) => eprintln!("forking task from {} at step {step}", src.display()),
        None => eprintln!("forking task from {}: {task:?}", src.display()),
    }
    let output = run_agent(
        provider,
        model,
        &task,
        &out_path,
        true,
        &[],
        None,
        None,
        SandboxPolicy::workspace_write(),
        None,
        &[],
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
        EventKind::PolicyDecision {
            tool_name,
            operation,
            decision,
            reason,
        } => {
            format!("POLICY   {tool_name} operation={operation} decision={decision}: {reason}")
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
    options: EvalRunOptions,
    report_json: Option<&Path>,
) -> Result<()> {
    let (text, all_passed) = eval_run_text(
        suite_path,
        provider,
        model,
        out_dir,
        base_url,
        &options,
        report_json,
    )
    .await?;
    print!("{text}");

    // CI gate:有 case 失败 → 退出码非 0(trace 已在 run_suite 内写完并 flush)。
    if !all_passed {
        std::process::exit(1);
    }
    Ok(())
}

async fn eval_run_text(
    suite_path: &Path,
    provider: &str,
    model: &str,
    out_dir: &Path,
    base_url: Option<&str>,
    options: &EvalRunOptions,
    report_json: Option<&Path>,
) -> Result<(String, bool)> {
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
    let report = run_suite_with_options(&suite, base_dir, &*p, model, out_dir, options).await?;
    if let Some(path) = report_json {
        write_eval_report_json(&report, provider, model, path)?;
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "eval: {} ({} case(s))",
        report.suite_name,
        report.total()
    ));
    lines.push(format!(
        "gate: {}",
        if report.gate_enabled { "on" } else { "off" }
    ));
    for r in &report.results {
        let tag = if r.passed { "PASS" } else { "FAIL" };
        lines.push(format!("[{tag}] {}  → {}", r.id, r.trace_path.display()));
    }
    let pct = (report.pass_rate() * 100.0).round() as u64;
    lines.push(format!(
        "{}/{} passed ({}%)",
        report.passed_count(),
        report.total(),
        pct
    ));
    let attempts_pct = (report.attempt_pass_rate() * 100.0).round() as u64;
    let (ci_low, ci_high) = wilson_ci95(report.attempts_passed(), report.attempts_total());
    lines.push(format!(
        "attempts: {}/{} passed ({}%, 95% CI {:.0}%–{:.0}%)",
        report.attempts_passed(),
        report.attempts_total(),
        attempts_pct,
        ci_low * 100.0,
        ci_high * 100.0
    ));
    for warning in &report.warnings {
        lines.push(format!("warning: {warning}"));
    }
    Ok((format!("{}\n", lines.join("\n")), report.all_passed()))
}

fn write_eval_report_json(
    report: &SuiteReport,
    provider: &str,
    model: &str,
    path: &Path,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let value = serde_json::json!({
        "schema_version": 1,
        "suite": report.suite_name,
        "provider": provider,
        "model": model,
        "gate_enabled": report.gate_enabled,
        "samples": report.samples,
        "cases_total": report.total(),
        "cases_passed": report.passed_count(),
        "attempts_total": report.attempts_total(),
        "attempts_passed": report.attempts_passed(),
        "pass_rate": report.attempt_pass_rate(),
        "ci95": ci_json(report.attempts_passed(), report.attempts_total()),
        "cases": report.results.iter().map(|r| serde_json::json!({
            "id": r.id,
            "passed": r.passed,
            "detail": r.detail,
            "trace": r.trace_path,
            "attempts_passed": report.attempts.iter().filter(|a| a.id == r.id && a.passed).count(),
            "attempts_total": report.attempts.iter().filter(|a| a.id == r.id).count(),
            "ci95": ci_json(
                report.attempts.iter().filter(|a| a.id == r.id && a.passed).count(),
                report.attempts.iter().filter(|a| a.id == r.id).count()
            ),
        })).collect::<Vec<_>>(),
        "attempts": report.attempts.iter().map(|r| serde_json::json!({
            "id": r.id,
            "sample": r.sample,
            "passed": r.passed,
            "detail": r.detail,
            "trace": r.trace_path,
        })).collect::<Vec<_>>(),
        "warnings": report.warnings,
    });
    std::fs::write(path, serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

fn ci_json(passed: usize, total: usize) -> serde_json::Value {
    let (low, high) = wilson_ci95(passed, total);
    serde_json::json!({ "low": low, "high": high })
}

fn wilson_ci95(passed: usize, total: usize) -> (f64, f64) {
    if total == 0 {
        return (0.0, 0.0);
    }
    let z = 1.959963984540054_f64;
    let n = total as f64;
    let phat = passed as f64 / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = phat + z2 / (2.0 * n);
    let margin = z * ((phat * (1.0 - phat) + z2 / (4.0 * n)) / n).sqrt();
    ((center - margin) / denom, (center + margin) / denom)
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
    let text = route_run_text(task, cheap, strong, verify, provider, trace_path, base_url).await?;
    print!("{text}");
    eprintln!("(trace written to {})", trace_path.display());
    Ok(())
}

async fn route_run_text(
    task: &str,
    cheap: &str,
    strong: &str,
    verify: &[String],
    provider: &str,
    trace_path: &Path,
    base_url: Option<&str>,
) -> Result<String> {
    if verify.is_empty() {
        anyhow::bail!("--verify is required for route (it decides whether to escalate)");
    }
    let work_dir = Path::new(".");

    let p = make_provider(provider, base_url)?;
    let report =
        run_with_escalation(&*p, cheap, strong, work_dir, trace_path, verify, task).await?;

    let mut lines = vec![report.final_text.clone()];
    let status = if report.passed { "passed" } else { "failed" };
    if report.escalated {
        lines.push(format!(
            "route: escalated {} → {} ({status})",
            report.cheap_model, report.strong_model
        ));
    } else {
        lines.push(format!(
            "route: stayed on {} ({status})",
            report.cheap_model
        ));
    }
    let actual = report.actual_cost();
    let saved = report.always_strong_cost - actual;
    lines.push(format!(
        "cost: ${:.4} actual (cheap ${:.4} + strong ${:.4}); vs always-strong ${:.4} → saved ${:.4}",
        actual, report.cheap_cost, report.strong_cost, report.always_strong_cost, saved
    ));
    Ok(format!("{}\n", lines.join("\n")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum McpServerTool {
    Verify,
    Eval,
    Route,
}

impl McpServerTool {
    fn parse(name: &str) -> Result<Self> {
        match name {
            "verify" => Ok(Self::Verify),
            "eval" => Ok(Self::Eval),
            "route" => Ok(Self::Route),
            other => {
                anyhow::bail!("unknown MCP server tool '{other}' (expected: verify | eval | route)")
            }
        }
    }
}

#[derive(Debug, Clone)]
struct McpServeOptions {
    workspace: PathBuf,
    allowed_tools: BTreeSet<McpServerTool>,
}

impl McpServeOptions {
    fn new(workspace: &Path, allow_tool: &[String]) -> Result<Self> {
        let workspace = workspace.canonicalize().map_err(|e| {
            anyhow::anyhow!("failed to resolve --workspace {}: {e}", workspace.display())
        })?;
        let mut allowed_tools = BTreeSet::from([McpServerTool::Verify]);
        for tool in allow_tool {
            allowed_tools.insert(McpServerTool::parse(tool)?);
        }
        Ok(Self {
            workspace,
            allowed_tools,
        })
    }

    fn allows(&self, name: &str) -> bool {
        McpServerTool::parse(name)
            .map(|tool| self.allowed_tools.contains(&tool))
            .unwrap_or(false)
    }

    fn tool_definitions(&self) -> Vec<serde_json::Value> {
        self.allowed_tools
            .iter()
            .map(|tool| mcp_tool_definition(*tool))
            .collect()
    }
}

fn mcp_tool_definition(tool: McpServerTool) -> serde_json::Value {
    match tool {
        McpServerTool::Verify => serde_json::json!({
            "name": "verify",
            "description": "Run verification commands (build/test/lint) in a directory; all must exit 0 to pass. Use this to prove a task is actually done before claiming success.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "commands": {"type": "array", "items": {"type": "string"}, "description": "Shell commands that must all exit 0."},
                    "dir": {"type": "string", "description": "Working directory relative to the MCP workspace (default: current)."}
                },
                "required": ["commands"]
            }
        }),
        McpServerTool::Eval => serde_json::json!({
            "name": "eval",
            "description": "Run an Argus eval suite and return its pass-rate. The suite cases may execute verification/reset shell commands.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "suite": {"type": "string", "description": "Path to the eval suite JSON file, relative to the MCP workspace."},
                    "provider": {"type": "string", "description": "Provider name (default: mock)."},
                    "model": {"type": "string", "description": "Model name (default: mock)."},
                    "out_dir": {"type": "string", "description": "Directory for per-case traces (default: .argus/eval)."},
                    "base_url": {"type": "string", "description": "Optional provider base URL."},
                    "samples": {"type": "integer", "description": "Repeated samples per case (default: 1)."},
                    "no_gate": {"type": "boolean", "description": "Disable agent self-repair gate; final verify still decides pass/fail."},
                    "in_place": {"type": "boolean", "description": "Run eval cases directly in their original directories instead of isolated temp workspaces."},
                    "report_json": {"type": "string", "description": "Optional path for a JSON report."}
                },
                "required": ["suite"]
            }
        }),
        McpServerTool::Route => serde_json::json!({
            "name": "route",
            "description": "Run cost-smart routing: cheap model first, escalate to strong when verification fails.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": {"type": "string", "description": "Task for the agent."},
                    "cheap": {"type": "string", "description": "Cheap model to try first."},
                    "strong": {"type": "string", "description": "Strong model to escalate to."},
                    "verify": {"type": "array", "items": {"type": "string"}, "description": "Verification commands that decide pass/escalate."},
                    "provider": {"type": "string", "description": "Provider name (default: mock)."},
                    "trace": {"type": "string", "description": "Trace output path (default: .argus/route.jsonl)."},
                    "base_url": {"type": "string", "description": "Optional provider base URL."}
                },
                "required": ["task", "cheap", "strong", "verify"]
            }
        }),
    }
}

/// Argus 作为 MCP server(stdio,newline JSON-RPC 2.0):把可靠性能力暴露成工具。
async fn mcp_serve(options: McpServeOptions) -> Result<()> {
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
                "result": {"tools": options.tool_definitions()}
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
                let call = if options.allows(name) {
                    mcp_serve_tool(name, &args, &options.workspace).await
                } else {
                    Err(anyhow::anyhow!(
                        "tool '{name}' is not allowed by this MCP server"
                    ))
                };
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
async fn mcp_serve_tool(name: &str, args: &serde_json::Value, workspace: &Path) -> Result<String> {
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
            let dir = resolve_mcp_tool_dir(workspace, Path::new(dir))?;
            let result = CommandVerifier::new(dir, commands).verify().await;
            Ok(format!("passed: {}\n{}", result.passed, result.detail))
        }
        "eval" => {
            let suite = args
                .get("suite")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("eval: 'suite' is required"))?;
            let provider = args
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("mock");
            let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("mock");
            let out_dir = args
                .get("out_dir")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".argus/eval"));
            let base_url = args.get("base_url").and_then(|v| v.as_str());
            let samples = args
                .get("samples")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(1);
            let no_gate = args
                .get("no_gate")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let in_place = args
                .get("in_place")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let report_json = args.get("report_json").and_then(|v| v.as_str());
            let suite_path = resolve_mcp_existing_path(workspace, Path::new(suite), "eval suite")?;
            let out_dir = resolve_mcp_output_path(workspace, &out_dir, "eval out_dir")?;
            let report_json_path = report_json
                .map(PathBuf::from)
                .map(|p| resolve_mcp_output_path(workspace, &p, "eval report_json"))
                .transpose()?;
            let options = EvalRunOptions {
                samples,
                gate_enabled: !no_gate,
                isolation: if in_place {
                    EvalIsolation::InPlace
                } else {
                    EvalIsolation::Isolated
                },
            };
            let (text, all_passed) = eval_run_text(
                &suite_path,
                provider,
                model,
                &out_dir,
                base_url,
                &options,
                report_json_path.as_deref(),
            )
            .await?;
            if !all_passed {
                anyhow::bail!("{text}");
            }
            Ok(text)
        }
        "route" => {
            let task = args
                .get("task")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("route: 'task' is required"))?;
            let cheap = args
                .get("cheap")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("route: 'cheap' is required"))?;
            let strong = args
                .get("strong")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("route: 'strong' is required"))?;
            let verify: Vec<String> = args
                .get("verify")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if verify.is_empty() {
                anyhow::bail!("route: 'verify' is required and must be non-empty");
            }
            let provider = args
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("mock");
            let trace = args
                .get("trace")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".argus/route.jsonl"));
            let trace = resolve_mcp_output_path(workspace, &trace, "route trace")?;
            let base_url = args.get("base_url").and_then(|v| v.as_str());
            route_run_text(task, cheap, strong, &verify, provider, &trace, base_url).await
        }
        other => anyhow::bail!("unknown tool '{other}'"),
    }
}

fn resolve_mcp_tool_dir(workspace: &Path, dir: &Path) -> Result<PathBuf> {
    let root = workspace.canonicalize()?;
    let candidate = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        root.join(dir)
    };
    let resolved = candidate
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("verify: failed to resolve dir {}: {e}", dir.display()))?;
    if !resolved.starts_with(&root) {
        anyhow::bail!(
            "verify: dir {} escapes MCP workspace {}",
            resolved.display(),
            root.display()
        );
    }
    Ok(resolved)
}

fn resolve_mcp_existing_path(workspace: &Path, path: &Path, label: &str) -> Result<PathBuf> {
    let root = workspace.canonicalize()?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let resolved = candidate
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("{label}: failed to resolve {}: {e}", path.display()))?;
    if !resolved.starts_with(&root) {
        anyhow::bail!(
            "{label}: path {} escapes MCP workspace {}",
            resolved.display(),
            root.display()
        );
    }
    Ok(resolved)
}

fn resolve_mcp_output_path(workspace: &Path, path: &Path, label: &str) -> Result<PathBuf> {
    let root = workspace.canonicalize()?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let parent = candidate.parent().unwrap_or(&root);
    let resolved_parent = parent.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "{label}: failed to resolve parent {}: {e}",
            parent.display()
        )
    })?;
    if !resolved_parent.starts_with(&root) {
        anyhow::bail!(
            "{label}: path {} escapes MCP workspace {}",
            candidate.display(),
            root.display()
        );
    }
    Ok(candidate)
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

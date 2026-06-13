use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_argus")
}

#[test]
fn run_then_show_roundtrip() {
    let dir = std::env::temp_dir().join(format!("argus-cli-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let trace = dir.join("trace.jsonl");

    let run = Command::new(bin())
        .args(["run", "make tea", "--trace"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(run.status.success(), "run failed: {run:?}");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("make tea"), "stdout was: {stdout}");

    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(show.status.success(), "show failed: {show:?}");
    let show_out = String::from_utf8_lossy(&show.stdout);
    assert!(show_out.contains("THOUGHT"), "show was: {show_out}");
    assert!(show_out.contains("MODEL"), "show was: {show_out}");
    assert!(show_out.contains("tokens)"), "MODEL line should show token counts: {show_out}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn show_empty_trace_prints_placeholder() {
    let dir = std::env::temp_dir().join(format!("argus-cli-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let trace = dir.join("empty.jsonl");
    std::fs::write(&trace, "").unwrap();
    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(show.status.success(), "show failed: {show:?}");
    let out = String::from_utf8_lossy(&show.stdout);
    assert!(out.contains("(empty trace)"), "show was: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_rejects_unknown_provider() {
    let trace = std::env::temp_dir().join(format!("argus-unkprov-{}.jsonl", std::process::id()));
    let out = Command::new(bin())
        .args(["run", "x", "--provider", "nope", "--trace"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(!out.status.success(), "should fail on unknown provider");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown provider"), "stderr was: {stderr}");
    let _ = std::fs::remove_file(&trace);
}

#[test]
fn run_anthropic_without_key_errors() {
    let trace = std::env::temp_dir().join(format!("argus-nokey-{}.jsonl", std::process::id()));
    let out = Command::new(bin())
        .args(["run", "x", "--provider", "anthropic", "--trace"])
        .arg(&trace)
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .unwrap();
    assert!(!out.status.success(), "should fail without key");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ANTHROPIC_API_KEY not set"), "stderr was: {stderr}");
    let _ = std::fs::remove_file(&trace);
}

#[test]
fn fork_reruns_task_from_trace() {
    let dir = std::env::temp_dir().join(format!("argus-fork-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let orig = dir.join("orig.jsonl");
    let forked = dir.join("forked.jsonl");

    let run = Command::new(bin())
        .args(["run", "brew coffee", "--trace"])
        .arg(&orig)
        .output()
        .unwrap();
    assert!(run.status.success());

    let fork = Command::new(bin())
        .args(["trace", "fork"])
        .arg(&orig)
        .args(["--provider", "mock", "--out"])
        .arg(&forked)
        .output()
        .unwrap();
    assert!(fork.status.success(), "fork failed: {fork:?}");
    let fork_out = String::from_utf8_lossy(&fork.stdout);
    assert!(fork_out.contains("brew coffee"), "fork stdout: {fork_out}");

    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&forked)
        .output()
        .unwrap();
    let show_out = String::from_utf8_lossy(&show.stdout);
    assert!(show_out.contains("TASK"), "show: {show_out}");
    assert!(show_out.contains("brew coffee"), "show: {show_out}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_with_yes_flag_succeeds() {
    let dir = std::env::temp_dir().join(format!("argus-yes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let trace = dir.join("trace.jsonl");

    let run = Command::new(bin())
        .args(["run", "x", "--yes", "--trace"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(run.status.success(), "run --yes failed: {run:?}");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains('x'), "stdout was: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn diff_compares_two_traces() {
    let dir = std::env::temp_dir().join(format!("argus-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.jsonl");
    let b = dir.join("b.jsonl");
    for (p, t) in [(&a, "task one"), (&b, "task two")] {
        let o = Command::new(bin()).args(["run", t, "--trace"]).arg(p).output().unwrap();
        assert!(o.status.success());
    }
    let diff = Command::new(bin()).args(["trace", "diff"]).arg(&a).arg(&b).output().unwrap();
    assert!(diff.status.success(), "diff failed: {diff:?}");
    let out = String::from_utf8_lossy(&diff.stdout);
    assert!(out.contains("task one"), "diff: {out}");
    assert!(out.contains("task two"), "diff: {out}");
    assert!(out.contains("≠"), "diff should mark differences: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_gate_blocks_fake_done() {
    let dir = std::env::temp_dir().join(format!("argus-vg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let trace = dir.join("t.jsonl");
    let out = Command::new(bin())
        .args(["run", "do x", "--yes", "--verify", "false", "--trace"])
        .arg(&trace)
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "should circuit-break, not crash: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("still failing"), "stdout: {stdout}");
    let show = Command::new(bin()).args(["trace","show"]).arg(&trace).output().unwrap();
    let s = String::from_utf8_lossy(&show.stdout);
    assert!(s.contains("GATE"), "show: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eval_runs_suite_and_reports_pass_rate() {
    let dir = std::env::temp_dir().join(format!("argus-eval-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let suite = dir.join("pass.json");
    std::fs::write(&suite, r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["true"]}]}"#).unwrap();

    let out = Command::new(bin())
        .args(["eval"])
        .arg(&suite)
        .args(["--out-dir"])
        .arg(dir.join("out"))
        .output()
        .unwrap();
    assert!(out.status.success(), "eval failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[PASS] a"), "stdout: {stdout}");
    assert!(stdout.contains("1/1 passed"), "stdout: {stdout}");
    assert!(dir.join("out").join("a.jsonl").exists(), "per-case trace missing");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eval_exits_nonzero_on_failure() {
    let dir = std::env::temp_dir().join(format!("argus-eval-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let suite = dir.join("fail.json");
    std::fs::write(&suite, r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["false"]}]}"#).unwrap();

    let out = Command::new(bin())
        .args(["eval"])
        .arg(&suite)
        .args(["--out-dir"])
        .arg(dir.join("out"))
        .output()
        .unwrap();
    assert!(!out.status.success(), "eval should exit non-zero when a case fails");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("0/1 passed"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn route_escalates_and_reports_cost() {
    let dir = std::env::temp_dir().join(format!("argus-route-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let trace = dir.join("t.jsonl");

    // verify "false" → cheap 失败 → 升级 strong
    let out = Command::new(bin())
        .args(["route", "do x"])
        .args(["--cheap", "claude-3-5-haiku-latest"])
        .args(["--strong", "claude-sonnet-4-5"])
        .args(["--verify", "false"])
        .arg("--trace")
        .arg(&trace)
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "route failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("escalated"), "stdout: {stdout}");

    // trace 含 ROUTE 行
    let show = Command::new(bin()).args(["trace", "show"]).arg(&trace).output().unwrap();
    let s = String::from_utf8_lossy(&show.stdout);
    assert!(s.contains("ROUTE"), "show: {s}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn route_requires_verify() {
    let dir = std::env::temp_dir().join(format!("argus-route-noverify-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new(bin())
        .args(["route", "do x"])
        .args(["--cheap", "claude-3-5-haiku-latest"])
        .args(["--strong", "claude-sonnet-4-5"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success(), "route without --verify should fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("verify"), "stderr should mention verify: {stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tui_errors_on_missing_trace() {
    let out = Command::new(bin())
        .args(["tui", "/nonexistent/argus-trace.jsonl"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "tui should fail on missing trace");
}

#[test]
fn run_openai_without_key_errors() {
    let trace = std::env::temp_dir().join(format!("argus-oai-nokey-{}.jsonl", std::process::id()));
    let out = Command::new(bin())
        .args(["run", "x", "--provider", "openai", "--trace"])
        .arg(&trace)
        .env_remove("OPENAI_API_KEY")
        .output()
        .unwrap();
    assert!(!out.status.success(), "should fail without key");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("OPENAI_API_KEY not set"), "stderr was: {stderr}");
    let _ = std::fs::remove_file(&trace);
}

#[test]
fn run_rejects_unknown_provider_lists_openai() {
    let trace = std::env::temp_dir().join(format!("argus-unk2-{}.jsonl", std::process::id()));
    let out = Command::new(bin())
        .args(["run", "x", "--provider", "nope", "--trace"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown provider"), "stderr: {stderr}");
    assert!(stderr.contains("openai"), "error should list openai as an option: {stderr}");
    let _ = std::fs::remove_file(&trace);
}

#[test]
fn run_auto_discovers_agents_md() {
    let dir = std::env::temp_dir().join(format!("argus-rules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("AGENTS.md"), "ALWAYS write Rust.").unwrap();
    let trace = dir.join("t.jsonl");

    let run = Command::new(bin())
        .args(["run", "do x", "--trace"])
        .arg(&trace)
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(run.status.success(), "run failed: {run:?}");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("AGENTS.md"), "should report loaded rules: {stderr}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_no_rules_flag_disables_discovery() {
    let dir = std::env::temp_dir().join(format!("argus-norules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("AGENTS.md"), "ALWAYS write Rust.").unwrap();
    let trace = dir.join("t.jsonl");

    let run = Command::new(bin())
        .args(["run", "do x", "--no-rules", "--trace"])
        .arg(&trace)
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!stderr.contains("AGENTS.md"), "should not load rules with --no-rules: {stderr}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_with_mcp_server_injects_and_calls_tool() {
    let dir = std::env::temp_dir().join(format!("argus-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let trace = dir.join("t.jsonl");

    // 用 argus 自己的 __mcp-mock 作为 MCP server;mock provider 会调第一个工具(echo)。
    let run = Command::new(bin())
        .args(["run", "use the tool", "--yes", "--mcp"])
        .arg(format!("{} __mcp-mock", bin()))
        .arg("--trace")
        .arg(&trace)
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(run.status.success(), "run --mcp failed: {run:?}");

    // trace 里应出现对 echo 工具的调用与结果
    let show = Command::new(bin()).args(["trace", "show"]).arg(&trace).output().unwrap();
    let s = String::from_utf8_lossy(&show.stdout);
    assert!(s.contains("echo"), "should have called the MCP echo tool: {s}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn mcp_serve_exposes_verify_tool() {
    use argus_core::McpClient;
    // Argus 当 MCP client,spawn Argus 当 MCP server
    let mut client = McpClient::spawn(bin(), &["mcp-serve".to_string()]).await.unwrap();
    let tools = client.list_tools().await.unwrap();
    assert!(tools.iter().any(|t| t.name == "verify"), "should expose 'verify' tool, got: {:?}", tools.iter().map(|t| &t.name).collect::<Vec<_>>());

    // verify ["true"] → 通过
    let out = client.call_tool("verify", &serde_json::json!({"commands": ["true"]})).await.unwrap();
    assert!(out.contains("passed: true"), "true should pass: {out}");

    // verify ["false"] → 不通过(含失败详情)
    let out = client.call_tool("verify", &serde_json::json!({"commands": ["false"]})).await.unwrap();
    assert!(out.contains("passed: false"), "false should not pass: {out}");
}

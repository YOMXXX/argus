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
    assert!(
        show_out.contains("tokens)"),
        "MODEL line should show token counts: {show_out}"
    );

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
    assert!(
        stderr.contains("ANTHROPIC_API_KEY not set"),
        "stderr was: {stderr}"
    );
    let _ = std::fs::remove_file(&trace);
}

#[test]
fn doctor_reports_basic_environment_checks() {
    let out = Command::new(bin()).arg("doctor").output().unwrap();

    assert!(out.status.success(), "doctor failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Argus doctor"), "stdout: {stdout}");
    assert!(stdout.contains("binary:"), "stdout: {stdout}");
    assert!(stdout.contains("git:"), "stdout: {stdout}");
    assert!(stdout.contains("providers:"), "stdout: {stdout}");
}

#[test]
fn policy_show_reports_sandbox_decisions() {
    let out = Command::new(bin())
        .args(["policy", "show", "--sandbox", "read-only"])
        .output()
        .unwrap();

    assert!(out.status.success(), "policy show failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("sandbox: read-only"), "stdout: {stdout}");
    assert!(stdout.contains("read_file"), "stdout: {stdout}");
    assert!(stdout.contains("write_file"), "stdout: {stdout}");
    assert!(stdout.contains("run_shell"), "stdout: {stdout}");
    assert!(stdout.contains("external_mcp"), "stdout: {stdout}");
    assert!(stdout.contains("deny"), "stdout: {stdout}");
}

#[test]
fn demo_runs_without_api_key_and_writes_trace() {
    let out = Command::new(bin()).arg("demo").output().unwrap();

    assert!(out.status.success(), "demo failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Argus demo"), "stdout: {stdout}");
    assert!(stdout.contains("verification passed"), "stdout: {stdout}");
    let trace_line = stdout
        .lines()
        .find(|line| line.starts_with("trace: "))
        .expect("demo should print trace path");
    let trace = trace_line.trim_start_matches("trace: ");
    assert!(
        std::path::Path::new(trace).exists(),
        "missing trace: {trace}"
    );
}

#[test]
fn demo_workspace_writes_deterministic_trace() {
    let dir = std::env::temp_dir().join(format!("argus-demo-workspace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let out = Command::new(bin())
        .args(["demo", "--workspace"])
        .arg(&dir)
        .output()
        .unwrap();

    assert!(out.status.success(), "demo failed: {out:?}");
    let trace = dir.join(".argus/demo.jsonl");
    assert!(trace.exists(), "missing trace: {}", trace.display());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(&format!("trace: {}", trace.display())),
        "stdout: {stdout}"
    );

    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(show.status.success(), "trace show failed: {show:?}");
    let show_out = String::from_utf8_lossy(&show.stdout);
    assert!(show_out.contains("GATE"), "show: {show_out}");

    let _ = std::fs::remove_dir_all(&dir);
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
fn fork_from_step_records_context_task() {
    let dir = std::env::temp_dir().join(format!("argus-fork-step-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let orig = dir.join("orig.jsonl");
    let forked = dir.join("forked.jsonl");

    let run = Command::new(bin())
        .args(["run", "brew tea", "--trace"])
        .arg(&orig)
        .output()
        .unwrap();
    assert!(run.status.success(), "run failed: {run:?}");

    let fork = Command::new(bin())
        .args(["trace", "fork"])
        .arg(&orig)
        .args(["--step", "1", "--provider", "mock", "--out"])
        .arg(&forked)
        .output()
        .unwrap();
    assert!(fork.status.success(), "fork failed: {fork:?}");
    let stderr = String::from_utf8_lossy(&fork.stderr);
    assert!(stderr.contains("step 1"), "stderr: {stderr}");

    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&forked)
        .output()
        .unwrap();
    let show_out = String::from_utf8_lossy(&show.stdout);
    assert!(
        show_out.contains("Fork context through step 1"),
        "show: {show_out}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fork_from_missing_step_errors() {
    let dir = std::env::temp_dir().join(format!("argus-fork-bad-step-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let orig = dir.join("orig.jsonl");
    let out = Command::new(bin())
        .args(["run", "x", "--trace"])
        .arg(&orig)
        .output()
        .unwrap();
    assert!(out.status.success());

    let fork = Command::new(bin())
        .args(["trace", "fork"])
        .arg(&orig)
        .args(["--step", "999"])
        .output()
        .unwrap();
    assert!(!fork.status.success(), "fork should fail for missing step");
    let stderr = String::from_utf8_lossy(&fork.stderr);
    assert!(stderr.contains("step 999"), "stderr: {stderr}");

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
        let o = Command::new(bin())
            .args(["run", t, "--trace"])
            .arg(p)
            .output()
            .unwrap();
        assert!(o.status.success());
    }
    let diff = Command::new(bin())
        .args(["trace", "diff"])
        .arg(&a)
        .arg(&b)
        .output()
        .unwrap();
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
    assert!(
        out.status.success(),
        "should circuit-break, not crash: {out:?}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("still failing"), "stdout: {stdout}");
    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&trace)
        .output()
        .unwrap();
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
    std::fs::write(
        &suite,
        r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["true"]}]}"#,
    )
    .unwrap();

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
    assert!(
        dir.join("out").join("a.jsonl").exists(),
        "per-case trace missing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eval_exits_nonzero_on_failure() {
    let dir = std::env::temp_dir().join(format!("argus-eval-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let suite = dir.join("fail.json");
    std::fs::write(
        &suite,
        r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["false"]}]}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["eval"])
        .arg(&suite)
        .args(["--out-dir"])
        .arg(dir.join("out"))
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "eval should exit non-zero when a case fails"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("0/1 passed"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eval_samples_and_report_json_are_supported() {
    let dir = std::env::temp_dir().join(format!("argus-eval-samples-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let suite = dir.join("suite.json");
    let report_json = dir.join("report.json");
    std::fs::write(
        &suite,
        r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["true"],"reset":{"command":"true"}}]}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["eval"])
        .arg(&suite)
        .args(["--samples", "2", "--report-json"])
        .arg(&report_json)
        .args(["--out-dir"])
        .arg(dir.join("out"))
        .output()
        .unwrap();

    assert!(out.status.success(), "eval failed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("gate: on"), "stdout: {stdout}");
    assert!(stdout.contains("attempts: 2/2 passed"), "stdout: {stdout}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_json).unwrap()).unwrap();
    assert_eq!(json["schema_version"], serde_json::json!(1));
    assert_eq!(json["samples"], serde_json::json!(2));
    assert_eq!(json["attempts_total"], serde_json::json!(2));
    assert!(json["ci95"]["low"].is_number(), "json: {json}");
    assert!(json["cases"][0]["ci95"]["high"].is_number(), "json: {json}");
    assert!(dir.join("out").join("a.sample-001.jsonl").exists());
    assert!(dir.join("out").join("a.sample-002.jsonl").exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eval_no_gate_still_exits_nonzero_on_failed_verify() {
    let dir = std::env::temp_dir().join(format!("argus-eval-no-gate-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let suite = dir.join("suite.json");
    std::fs::write(
        &suite,
        r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["false"]}]}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["eval"])
        .arg(&suite)
        .args(["--no-gate"])
        .args(["--out-dir"])
        .arg(dir.join("out"))
        .output()
        .unwrap();

    assert!(!out.status.success(), "eval --no-gate should fail");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("gate: off"), "stdout: {stdout}");
    assert!(stdout.contains("0/1 passed"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eval_in_place_allows_case_mutation() {
    let dir = std::env::temp_dir().join(format!("argus-eval-in-place-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("case")).unwrap();
    std::fs::write(dir.join("case").join("state.txt"), "dirty").unwrap();
    let suite = dir.join("suite.json");
    std::fs::write(
        &suite,
        r#"{"name":"s","cases":[{"id":"a","dir":"case","task":"do","verify":["test \"$(cat state.txt)\" = clean"],"reset":{"command":"printf clean > state.txt"}}]}"#,
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["eval"])
        .arg(&suite)
        .args(["--out-dir"])
        .arg(dir.join("out"))
        .arg("--in-place")
        .output()
        .unwrap();

    assert!(out.status.success(), "eval failed: {out:?}");
    assert_eq!(
        std::fs::read_to_string(dir.join("case").join("state.txt")).unwrap(),
        "clean"
    );
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
    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&trace)
        .output()
        .unwrap();
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
    assert!(
        stderr.contains("verify"),
        "stderr should mention verify: {stderr}"
    );
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
    assert!(
        stderr.contains("OPENAI_API_KEY not set"),
        "stderr was: {stderr}"
    );
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
    assert!(
        stderr.contains("openai"),
        "error should list openai as an option: {stderr}"
    );
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
    assert!(
        stderr.contains("AGENTS.md"),
        "should report loaded rules: {stderr}"
    );

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
    assert!(
        !stderr.contains("AGENTS.md"),
        "should not load rules with --no-rules: {stderr}"
    );

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
        .args([
            "run",
            "use the tool",
            "--yes",
            "--mcp-allow",
            "echo",
            "--mcp",
        ])
        .arg(format!("{} __mcp-mock", bin()))
        .arg("--trace")
        .arg(&trace)
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(run.status.success(), "run --mcp failed: {run:?}");

    // trace 里应出现对 echo 工具的调用与结果
    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&trace)
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&show.stdout);
    assert!(
        s.contains("echo"),
        "should have called the MCP echo tool: {s}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_with_yes_and_mcp_requires_allowlist() {
    let dir = std::env::temp_dir().join(format!("argus-mcp-allowlist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let trace = dir.join("t.jsonl");

    let run = Command::new(bin())
        .args(["run", "use the tool", "--yes", "--mcp"])
        .arg(format!("{} __mcp-mock", bin()))
        .arg("--trace")
        .arg(&trace)
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(!run.status.success(), "run should fail without allowlist");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("--mcp-allow"),
        "stderr should explain allowlist requirement: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_read_only_sandbox_denies_mcp_even_with_yes() {
    let dir = std::env::temp_dir().join(format!("argus-readonly-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let trace = dir.join("trace.jsonl");
    let mcp_cmd = format!("{} __mcp-mock", bin());

    let run = Command::new(bin())
        .args([
            "run",
            "call external tool",
            "--sandbox",
            "read-only",
            "--yes",
            "--mcp-allow",
            "echo",
            "--mcp",
            &mcp_cmd,
            "--trace",
        ])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(run.status.success(), "run failed: {run:?}");

    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(show.status.success(), "show failed: {show:?}");
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(
        stdout.contains("POLICY"),
        "trace should show policy: {stdout}"
    );
    assert!(stdout.contains("deny"), "policy should deny MCP: {stdout}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn mcp_serve_exposes_verify_tool() {
    use argus_core::McpClient;
    // Argus 当 MCP client,spawn Argus 当 MCP server
    let mut client = McpClient::spawn(bin(), &["mcp-serve".to_string()])
        .await
        .unwrap();
    let tools = client.list_tools().await.unwrap();
    let names = tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>();
    assert!(names.contains(&"verify"), "tools: {names:?}");
    assert!(!names.contains(&"eval"), "eval should be opt-in: {names:?}");
    assert!(
        !names.contains(&"route"),
        "route should be opt-in: {names:?}"
    );

    // verify ["true"] → 通过
    let out = client
        .call_tool("verify", &serde_json::json!({"commands": ["true"]}))
        .await
        .unwrap();
    assert!(out.contains("passed: true"), "true should pass: {out}");

    // verify ["false"] → 不通过(含失败详情)
    let out = client
        .call_tool("verify", &serde_json::json!({"commands": ["false"]}))
        .await
        .unwrap();
    assert!(
        out.contains("passed: false"),
        "false should not pass: {out}"
    );
}

#[tokio::test]
async fn mcp_serve_requires_allow_tool_for_eval_and_route() {
    use argus_core::McpClient;

    let mut default_client = McpClient::spawn(bin(), &["mcp-serve".to_string()])
        .await
        .unwrap();
    let out = default_client
        .call_tool("eval", &serde_json::json!({"suite": "missing-suite.json"}))
        .await
        .unwrap();
    assert!(
        out.contains("not allowed"),
        "eval should be blocked by default: {out}"
    );

    let mut allowed_client = McpClient::spawn(
        bin(),
        &[
            "mcp-serve".to_string(),
            "--allow-tool".to_string(),
            "eval".to_string(),
            "--allow-tool".to_string(),
            "route".to_string(),
        ],
    )
    .await
    .unwrap();
    let tools = allowed_client.list_tools().await.unwrap();
    let names = tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>();
    assert!(names.contains(&"verify"), "tools: {names:?}");
    assert!(names.contains(&"eval"), "tools: {names:?}");
    assert!(names.contains(&"route"), "tools: {names:?}");
}

#[tokio::test]
async fn mcp_serve_verify_rejects_dir_outside_workspace() {
    use argus_core::McpClient;

    let workspace =
        std::env::temp_dir().join(format!("argus-mcp-workspace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workspace);
    std::fs::create_dir_all(&workspace).unwrap();
    let outside = std::env::temp_dir().join(format!("argus-mcp-outside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).unwrap();

    let mut client = McpClient::spawn(
        bin(),
        &[
            "mcp-serve".to_string(),
            "--workspace".to_string(),
            workspace.to_string_lossy().to_string(),
        ],
    )
    .await
    .unwrap();
    let out = client
        .call_tool(
            "verify",
            &serde_json::json!({"commands":["true"], "dir": outside}),
        )
        .await
        .unwrap();

    assert!(
        out.contains("error:"),
        "tool should return an MCP error: {out}"
    );
    assert!(
        out.contains("escapes MCP workspace"),
        "error should explain the boundary: {out}"
    );

    let _ = std::fs::remove_dir_all(&workspace);
    let _ = std::fs::remove_dir_all(&outside);
}

#[tokio::test]
async fn mcp_serve_eval_runs_suite() {
    use argus_core::McpClient;

    let dir = std::env::temp_dir().join(format!("argus-mcp-eval-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let suite = dir.join("suite.json");
    std::fs::write(
        &suite,
        r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["true"]}]}"#,
    )
    .unwrap();

    let mut client = McpClient::spawn(
        bin(),
        &[
            "mcp-serve".to_string(),
            "--workspace".to_string(),
            dir.to_string_lossy().to_string(),
            "--allow-tool".to_string(),
            "eval".to_string(),
        ],
    )
    .await
    .unwrap();
    let out = client
        .call_tool(
            "eval",
            &serde_json::json!({
                "suite": "suite.json",
                "out_dir": "out"
            }),
        )
        .await
        .unwrap();

    assert!(out.contains("1/1 passed"), "out: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn mcp_serve_eval_accepts_sampling_options() {
    use argus_core::McpClient;

    let dir = std::env::temp_dir().join(format!("argus-mcp-eval-samples-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let suite = dir.join("suite.json");
    std::fs::write(
        &suite,
        r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["true"],"reset":{"command":"true"}}]}"#,
    )
    .unwrap();

    let mut client = McpClient::spawn(
        bin(),
        &[
            "mcp-serve".to_string(),
            "--workspace".to_string(),
            dir.to_string_lossy().to_string(),
            "--allow-tool".to_string(),
            "eval".to_string(),
        ],
    )
    .await
    .unwrap();
    let out = client
        .call_tool(
            "eval",
            &serde_json::json!({
                "suite": "suite.json",
                "out_dir": "out",
                "samples": 2,
                "no_gate": true
            }),
        )
        .await
        .unwrap();

    assert!(out.contains("gate: off"), "out: {out}");
    assert!(out.contains("attempts: 2/2 passed"), "out: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn mcp_serve_route_runs_mock_route() {
    use argus_core::McpClient;

    let dir = std::env::temp_dir().join(format!("argus-mcp-route-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut client = McpClient::spawn(
        bin(),
        &[
            "mcp-serve".to_string(),
            "--workspace".to_string(),
            dir.to_string_lossy().to_string(),
            "--allow-tool".to_string(),
            "route".to_string(),
        ],
    )
    .await
    .unwrap();
    let out = client
        .call_tool(
            "route",
            &serde_json::json!({
                "task": "do x",
                "cheap": "cheap-model",
                "strong": "strong-model",
                "verify": ["true"],
                "trace": "route.jsonl"
            }),
        )
        .await
        .unwrap();

    assert!(out.contains("route:"), "out: {out}");
    assert!(out.contains("cost:"), "out: {out}");
    let _ = std::fs::remove_dir_all(&dir);
}

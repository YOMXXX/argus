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

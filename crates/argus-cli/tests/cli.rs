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
    assert!(run.status.success(), "run failed: {:?}", run);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("make tea"), "stdout was: {stdout}");

    let show = Command::new(bin())
        .args(["trace", "show"])
        .arg(&trace)
        .output()
        .unwrap();
    assert!(show.status.success(), "show failed: {:?}", show);
    let show_out = String::from_utf8_lossy(&show.stdout);
    assert!(show_out.contains("THOUGHT"), "show was: {show_out}");
    assert!(show_out.contains("MODEL"), "show was: {show_out}");

    let _ = std::fs::remove_dir_all(&dir);
}

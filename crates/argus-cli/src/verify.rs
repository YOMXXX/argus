use anyhow::Result;
use argus_core::{CommandVerifier, Verifier};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyRunOutput {
    pub commands: Vec<String>,
    pub passed: bool,
    pub detail: String,
}

pub fn run_configured_verify(root: &Path, commands: &[String]) -> Result<VerifyRunOutput> {
    if commands.is_empty() {
        return Ok(VerifyRunOutput {
            commands: Vec::new(),
            passed: false,
            detail: "no verify command configured".into(),
        });
    }

    let root = root.to_path_buf();
    let commands = commands.to_vec();
    let run_commands = commands.clone();
    let handle = std::thread::spawn(move || run_verify_in_runtime(root, run_commands));
    let result = handle
        .join()
        .map_err(|_| anyhow::anyhow!("verification runner panicked"))??;

    Ok(VerifyRunOutput {
        commands,
        passed: result.passed,
        detail: result.detail,
    })
}

fn run_verify_in_runtime(root: PathBuf, commands: Vec<String>) -> Result<argus_core::VerifyResult> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(runtime.block_on(CommandVerifier::new(root, commands).verify()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "arguscode-verify-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn run_configured_verify_reports_passed_gate() {
        let dir = temp_dir("pass");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("marker.txt"), "ok\n").unwrap();

        let output = run_configured_verify(&dir, &["test -f marker.txt".into()]).unwrap();

        assert_eq!(output.commands, vec!["test -f marker.txt"]);
        assert!(output.passed, "{output:?}");
        assert_eq!(output.detail, "1 check(s) passed");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

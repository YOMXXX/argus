use crate::tasks::{update_task_status, TaskRecord};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessRunOutput {
    pub task_id: String,
    pub task_text: String,
    pub status: String,
    pub trace: PathBuf,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_task_through_harness(root: &Path, record: &TaskRecord) -> Result<HarnessRunOutput> {
    let (_, config) = crate::workbench::ensure_config(root)?;
    update_task_status(root, &record.id, "running")?;
    let trace = PathBuf::from(".argus/tasks").join(format!("{}.trace.jsonl", record.id));

    let mut command = Command::new(argus_binary_path()?);
    command
        .current_dir(root)
        .arg("run")
        .arg(&record.text)
        .arg("--provider")
        .arg(&config.provider.default_provider)
        .arg("--model")
        .arg(&config.provider.default_model)
        .arg("--yes")
        .arg("--trace")
        .arg(&trace);
    for verify in &config.verify.commands {
        command.arg("--verify").arg(verify);
    }

    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        update_task_status(root, &record.id, "done")?;
        Ok(HarnessRunOutput {
            task_id: record.id.clone(),
            task_text: record.text.clone(),
            status: "done".into(),
            trace,
            stdout,
            stderr,
        })
    } else {
        update_task_status(root, &record.id, "failed")?;
        anyhow::bail!(
            "Argus harness failed with {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            output.status,
            stdout,
            stderr
        )
    }
}

pub fn argus_binary_path() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let binary_name = if cfg!(windows) { "argus.exe" } else { "argus" };
    let sibling = exe.with_file_name(binary_name);
    if sibling.exists() {
        Ok(sibling)
    } else {
        Ok(PathBuf::from(binary_name))
    }
}

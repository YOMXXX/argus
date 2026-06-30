use anyhow::Result;
use argus_cli::project::{detect_project, init_project, init_report_text};
use argus_cli::tasks::{
    latest_resumable_task, list_tasks, queue_task, update_task_status, TaskRecord,
};
use argus_cli::workbench::{ensure_config, run_workbench};
use argus_core::{CommandVerifier, Verifier};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(
    name = "arguscode",
    version,
    about = "ArgusCode — an open AI coding workbench powered by the Argus harness."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    /// Print a non-interactive project summary instead of opening the TUI.
    #[arg(long)]
    status: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize ArgusCode in the current repository.
    Init {
        /// Rewrite existing .argus/config.toml, memory, and smoke eval files.
        #[arg(long)]
        force: bool,
    },
    /// Print project detection and ArgusCode config status.
    Status,
    /// Open the full-screen ArgusCode Workbench TUI.
    Workbench,
    /// Open the coding chat/workbench, optionally queueing an initial task.
    Chat {
        /// Initial task to place in the ArgusCode task queue.
        task: Option<String>,
    },
    /// Queue a task or list queued tasks.
    Task {
        /// Task text to queue. Omit to list the queue.
        task: Option<String>,
    },
    /// Resume the latest queued task.
    Resume {
        /// Execute the task through the Argus harness and write a trace.
        #[arg(long)]
        run: bool,
    },
    /// Run the configured project verification gate.
    Verify,
    /// Check local project readiness for ArgusCode.
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    if cli.status {
        return status(&cwd);
    }
    match cli.command.unwrap_or(Commands::Workbench) {
        Commands::Init { force } => {
            let report = init_project(&cwd, force)?;
            println!("{}", init_report_text(&report));
            Ok(())
        }
        Commands::Status => status(&cwd),
        Commands::Workbench => run_workbench(&cwd),
        Commands::Chat { task } => chat(&cwd, task),
        Commands::Task { task } => task_command(&cwd, task),
        Commands::Resume { run } => resume(&cwd, run),
        Commands::Verify => verify(&cwd).await,
        Commands::Doctor => doctor(&cwd),
    }
}

fn chat(cwd: &std::path::Path, task: Option<String>) -> Result<()> {
    if let Some(task) = task {
        let (profile, _) = ensure_config(cwd)?;
        let record = queue_task(&profile.root, &task)?;
        println!("Queued task {}: {}", record.id, record.text);
        println!("Opening ArgusCode Workbench...");
    }
    run_workbench(cwd)
}

fn task_command(cwd: &std::path::Path, task: Option<String>) -> Result<()> {
    let (profile, _) = ensure_config(cwd)?;
    if let Some(task) = task {
        let record = queue_task(&profile.root, &task)?;
        println!("Queued task {}: {}", record.id, record.text);
        println!("status: {}", record.status);
        println!("queue: .argus/tasks/queue.jsonl");
        return Ok(());
    }

    let tasks = list_tasks(&profile.root)?;
    println!("ArgusCode task queue");
    if tasks.is_empty() {
        println!("(empty)");
    } else {
        for record in tasks {
            println!("[{}] {}  {}", record.status, record.id, record.text);
        }
    }
    Ok(())
}

fn resume(cwd: &std::path::Path, run: bool) -> Result<()> {
    let (profile, _) = ensure_config(cwd)?;
    match latest_resumable_task(&profile.root)? {
        Some(record) => {
            println!("Resuming task {}: {}", record.id, record.text);
            println!("status: {}", record.status);
            if run {
                run_task_through_harness(&profile.root, &record)?;
            } else {
                println!("Open the TUI with: arguscode");
                println!("Run it through the harness with: arguscode resume --run");
            }
        }
        None => {
            println!("No resumable task found.");
            println!("Queue one with: arguscode task \"fix the failing test\"");
        }
    }
    Ok(())
}

fn run_task_through_harness(root: &std::path::Path, record: &TaskRecord) -> Result<()> {
    let (_, config) = ensure_config(root)?;
    update_task_status(root, &record.id, "running")?;
    let trace = PathBuf::from(".argus/tasks").join(format!("{}.trace.jsonl", record.id));

    println!("Running task through Argus harness: {}", record.text);
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
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));

    if output.status.success() {
        update_task_status(root, &record.id, "done")?;
        println!("status: done");
        println!("trace: {}", trace.display());
        Ok(())
    } else {
        update_task_status(root, &record.id, "failed")?;
        anyhow::bail!("Argus harness failed with {}", output.status)
    }
}

fn argus_binary_path() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let binary_name = if cfg!(windows) { "argus.exe" } else { "argus" };
    let sibling = exe.with_file_name(binary_name);
    if sibling.exists() {
        Ok(sibling)
    } else {
        Ok(PathBuf::from(binary_name))
    }
}

async fn verify(cwd: &std::path::Path) -> Result<()> {
    let (profile, config) = ensure_config(cwd)?;
    println!("ArgusCode verify");
    for command in &config.verify.commands {
        println!("$ {command}");
    }
    let result = CommandVerifier::new(&profile.root, config.verify.commands.clone())
        .verify()
        .await;
    println!("{}", result.detail);
    if result.passed {
        println!("verification passed");
        Ok(())
    } else {
        println!("verification failed");
        anyhow::bail!("verification failed")
    }
}

fn status(cwd: &std::path::Path) -> Result<()> {
    let (profile, config) = ensure_config(cwd)?;
    println!("ArgusCode status");
    println!("project: {}", profile.name);
    println!("root: {}", profile.root.display());
    println!(
        "languages: {}",
        if profile.languages.is_empty() {
            "unknown".into()
        } else {
            profile.languages.join(", ")
        }
    );
    println!(
        "package manager: {}",
        profile.package_manager.as_deref().unwrap_or("unknown")
    );
    println!(
        "provider: {}/{}",
        config.provider.default_provider, config.provider.default_model
    );
    println!("gate: {}", if config.verify.gate { "on" } else { "off" });
    println!("verify:");
    for command in &config.verify.commands {
        println!("  - {command}");
    }
    println!("config: {}", PathBuf::from(".argus/config.toml").display());
    Ok(())
}

fn doctor(cwd: &std::path::Path) -> Result<()> {
    let profile = detect_project(cwd)?;
    println!("ArgusCode doctor");
    println!("binary: arguscode {}", env!("CARGO_PKG_VERSION"));
    println!("project: {}", profile.name);
    println!("root: {}", profile.root.display());
    println!(
        "git: {}",
        if command_available("git") {
            "available"
        } else {
            "missing"
        }
    );
    println!(
        "config: {}",
        if profile.root.join(".argus/config.toml").exists() {
            "present"
        } else {
            "missing; run arguscode init"
        }
    );
    println!(
        "rules: {}",
        if profile.rules_files.is_empty() {
            "none detected".into()
        } else {
            profile
                .rules_files
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    Ok(())
}

fn command_available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

use anyhow::Result;
use argus_cli::harness::{run_task_through_harness, HarnessRunOutput};
use argus_cli::project::{detect_project, init_project, init_report_text};
use argus_cli::tasks::{latest_resumable_task, list_tasks, queue_task, TaskRecord};
use argus_cli::workbench::{ensure_config, run_workbench};
use argus_core::{CommandVerifier, Verifier};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
    /// Show or update the default model/provider profile.
    Provider {
        #[command(subcommand)]
        command: Option<ProviderCommands>,
    },
    /// Check local project readiness for ArgusCode.
    Doctor,
}

#[derive(Subcommand)]
enum ProviderCommands {
    /// Use the built-in mock provider.
    Mock,
    /// Use OpenAI with OPENAI_API_KEY.
    Openai {
        /// Model name to use.
        #[arg(long, default_value = "gpt-4o-mini")]
        model: String,
    },
    /// Use DeepSeek through the OpenAI-compatible API.
    Deepseek {
        /// Model name to use.
        #[arg(long, default_value = "deepseek-chat")]
        model: String,
    },
    /// Use any OpenAI-compatible endpoint.
    Custom {
        /// Provider adapter name. Use `openai` for OpenAI-compatible APIs.
        #[arg(long, default_value = "openai")]
        provider: String,
        /// Model name to use.
        #[arg(long)]
        model: String,
        /// Base URL for the provider API.
        #[arg(long = "base-url")]
        base_url: Option<String>,
        /// Environment variable that stores the API key.
        #[arg(long = "api-key-env")]
        api_key_env: Option<String>,
    },
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
        Commands::Provider { command } => provider_command(&cwd, command),
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
                run_task_through_harness_command(&profile.root, &record)?;
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

fn run_task_through_harness_command(root: &std::path::Path, record: &TaskRecord) -> Result<()> {
    println!("Running task through Argus harness: {}", record.text);
    print_harness_output(run_task_through_harness(root, record)?);
    Ok(())
}

fn print_harness_output(output: HarnessRunOutput) {
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    println!("status: {}", output.status);
    println!("trace: {}", output.trace.display());
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

fn provider_command(cwd: &std::path::Path, command: Option<ProviderCommands>) -> Result<()> {
    let (profile, mut config) = ensure_config(cwd)?;
    match command {
        Some(ProviderCommands::Mock) => {
            config.provider.default_provider = "mock".into();
            config.provider.default_model = "mock".into();
            config.provider.base_url = None;
            config.provider.api_key_env = None;
            config.provider.routing = "manual".into();
            config.write(&profile.root)?;
        }
        Some(ProviderCommands::Openai { model }) => {
            config.provider.default_provider = "openai".into();
            config.provider.default_model = model;
            config.provider.base_url = None;
            config.provider.api_key_env = Some("OPENAI_API_KEY".into());
            config.provider.routing = "manual".into();
            config.write(&profile.root)?;
        }
        Some(ProviderCommands::Deepseek { model }) => {
            config.provider.default_provider = "openai".into();
            config.provider.default_model = model;
            config.provider.base_url = Some("https://api.deepseek.com".into());
            config.provider.api_key_env = Some("DEEPSEEK_API_KEY".into());
            config.provider.routing = "manual".into();
            config.write(&profile.root)?;
        }
        Some(ProviderCommands::Custom {
            provider,
            model,
            base_url,
            api_key_env,
        }) => {
            config.provider.default_provider = provider;
            config.provider.default_model = model;
            config.provider.base_url = base_url;
            config.provider.api_key_env = api_key_env;
            config.provider.routing = "manual".into();
            config.write(&profile.root)?;
        }
        None => {}
    }
    print_provider(&config);
    Ok(())
}

fn print_provider(config: &argus_cli::config::ArgusCodeConfig) {
    println!("ArgusCode provider");
    println!("provider: {}", config.provider.default_provider);
    println!("model: {}", config.provider.default_model);
    if let Some(base_url) = &config.provider.base_url {
        println!("base url: {base_url}");
    }
    if let Some(api_key_env) = &config.provider.api_key_env {
        println!("api key env: {api_key_env}");
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
    if let Some(base_url) = &config.provider.base_url {
        println!("base url: {base_url}");
    }
    if let Some(api_key_env) = &config.provider.api_key_env {
        println!("api key env: {api_key_env}");
    }
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

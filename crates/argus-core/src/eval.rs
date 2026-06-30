//! Eval 引擎:在仓库上批量量化 agent 的通过率/回归。
//!
//! 一个 case = task + 一组 verify 命令;pass 判据 = 跑完 agent 后 verify 全部 exit 0。

use crate::agent::Agent;
use crate::approver::AutoApprover;
use crate::command::{CommandRunner, ExecutionLimits};
use crate::policy::SandboxPolicy;
use crate::provider::Provider;
use crate::tool::{ReadFile, RunShell, WriteFile};
use crate::verifier::{CommandVerifier, Verifier, VerifyResult};
use argus_trace::{EventKind, TraceWriter};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 一份 eval 套件:一组 case。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSuite {
    pub name: String,
    pub cases: Vec<EvalCase>,
}

/// 单个 eval case。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    /// 文件名安全的 slug,用作该 case 的 trace 文件名。
    pub id: String,
    /// 交给 agent 的任务。
    pub task: String,
    /// case 工作目录,相对 suite 文件所在目录;None → suite 所在目录。
    #[serde(default)]
    pub dir: Option<String>,
    /// 须全部 exit 0 才算通过的校验命令。
    #[serde(default)]
    pub verify: Vec<String>,
    /// 可选的 case 重置策略。省略时保持原有“原地运行、不重置”的行为。
    #[serde(default)]
    pub reset: Option<EvalReset>,
}

/// 单个 eval case 的重置策略。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "EvalResetRepr", into = "EvalResetRepr")]
pub enum EvalReset {
    Git,
    Command { command: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum EvalResetRepr {
    String(String),
    Command { command: String },
}

impl TryFrom<EvalResetRepr> for EvalReset {
    type Error = String;

    fn try_from(value: EvalResetRepr) -> Result<Self, Self::Error> {
        match value {
            EvalResetRepr::String(s) if s == "git" => Ok(Self::Git),
            EvalResetRepr::String(s) => Err(format!("unknown eval reset mode '{s}'")),
            EvalResetRepr::Command { command } if command.trim().is_empty() => {
                Err("eval reset command must not be empty".into())
            }
            EvalResetRepr::Command { command } => Ok(Self::Command { command }),
        }
    }
}

impl From<EvalReset> for EvalResetRepr {
    fn from(value: EvalReset) -> Self {
        match value {
            EvalReset::Git => Self::String("git".into()),
            EvalReset::Command { command } => Self::Command { command },
        }
    }
}

/// 单个 case 的运行结果。
#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub passed: bool,
    pub detail: String,
    pub trace_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalIsolation {
    Isolated,
    InPlace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalRunOptions {
    pub samples: usize,
    pub gate_enabled: bool,
    pub isolation: EvalIsolation,
}

impl Default for EvalRunOptions {
    fn default() -> Self {
        Self {
            samples: 1,
            gate_enabled: true,
            isolation: EvalIsolation::Isolated,
        }
    }
}

/// 单个 case 的一次 sample/attempt 结果。
#[derive(Debug, Clone, Serialize)]
pub struct AttemptResult {
    pub id: String,
    pub sample: usize,
    pub passed: bool,
    pub detail: String,
    pub trace_path: PathBuf,
}

/// 整个套件的报告。
#[derive(Debug, Clone, Serialize)]
pub struct SuiteReport {
    pub suite_name: String,
    pub results: Vec<CaseResult>,
    pub attempts: Vec<AttemptResult>,
    pub samples: usize,
    pub gate_enabled: bool,
    pub warnings: Vec<String>,
}

impl SuiteReport {
    pub fn total(&self) -> usize {
        self.results.len()
    }
    pub fn passed_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }
    pub fn attempts_total(&self) -> usize {
        self.attempts.len()
    }
    pub fn attempts_passed(&self) -> usize {
        self.attempts.iter().filter(|r| r.passed).count()
    }
    pub fn attempt_pass_rate(&self) -> f64 {
        if self.attempts.is_empty() {
            return 0.0;
        }
        self.attempts_passed() as f64 / self.attempts_total() as f64
    }
    /// 通过率 [0.0, 1.0];空套件返回 0.0。
    pub fn pass_rate(&self) -> f64 {
        if self.results.is_empty() {
            return 0.0;
        }
        self.passed_count() as f64 / self.total() as f64
    }
}

/// 跑完整套件:逐个 case 跑 agent 并独立裁决,返回报告。
///
/// `base_dir` = suite 文件所在目录(解析每个 case 的相对 `dir`)。
/// 单个 case 内部出错记为 `passed=false` 并继续;仅在套件级致命错误(如
/// `out_dir` 无法创建)时返回 `Err`。
pub async fn run_suite(
    suite: &EvalSuite,
    base_dir: &Path,
    provider: &dyn Provider,
    model: &str,
    out_dir: &Path,
) -> anyhow::Result<SuiteReport> {
    run_suite_with_options(
        suite,
        base_dir,
        provider,
        model,
        out_dir,
        &EvalRunOptions::default(),
    )
    .await
}

pub async fn run_suite_with_options(
    suite: &EvalSuite,
    base_dir: &Path,
    provider: &dyn Provider,
    model: &str,
    out_dir: &Path,
    options: &EvalRunOptions,
) -> anyhow::Result<SuiteReport> {
    if options.samples == 0 {
        anyhow::bail!("eval samples must be at least 1");
    }
    std::fs::create_dir_all(out_dir)?;
    let mut results = Vec::with_capacity(suite.cases.len());
    let mut attempts = Vec::new();
    let mut warnings = Vec::new();
    for case in &suite.cases {
        let case_rel = Path::new(case.dir.as_deref().unwrap_or("."));
        let case_dir = base_dir.join(case_rel);
        if options.isolation == EvalIsolation::InPlace
            && options.samples > 1
            && case.reset.is_none()
        {
            warnings.push(format!(
                "case '{}' has samples={} without reset; samples may share state",
                case.id, options.samples
            ));
        }
        let mut case_attempts = Vec::with_capacity(options.samples);
        for sample in 1..=options.samples {
            let trace_path = trace_path_for_sample(out_dir, &case.id, options.samples, sample);
            let _ = std::fs::remove_file(&trace_path);
            let ctx = CaseRunContext {
                case,
                base_dir,
                case_rel,
                case_dir: &case_dir,
                trace_path: &trace_path,
                provider,
                model,
                gate_enabled: options.gate_enabled,
            };
            let run = match options.isolation {
                EvalIsolation::InPlace => run_one_case_with_reset(ctx).await,
                EvalIsolation::Isolated => run_one_case_isolated(ctx).await,
            };
            let attempt = match run {
                Ok(vr) => AttemptResult {
                    id: case.id.clone(),
                    sample,
                    passed: vr.passed,
                    detail: vr.detail,
                    trace_path,
                },
                Err(e) => AttemptResult {
                    id: case.id.clone(),
                    sample,
                    passed: false,
                    detail: format!("case error: {e}"),
                    trace_path,
                },
            };
            attempts.push(attempt.clone());
            case_attempts.push(attempt);
        }
        let passed = case_attempts.iter().all(|r| r.passed);
        let detail = case_attempts
            .iter()
            .map(|r| format!("sample {}: {}", r.sample, r.detail))
            .collect::<Vec<_>>()
            .join("\n");
        let trace_path = case_attempts
            .last()
            .map(|r| r.trace_path.clone())
            .unwrap_or_else(|| trace_path_for_sample(out_dir, &case.id, options.samples, 1));
        let case_result = CaseResult {
            id: case.id.clone(),
            passed,
            detail,
            trace_path,
        };
        results.push(case_result);
    }
    Ok(SuiteReport {
        suite_name: suite.name.clone(),
        results,
        attempts,
        samples: options.samples,
        gate_enabled: options.gate_enabled,
        warnings,
    })
}

fn trace_path_for_sample(out_dir: &Path, case_id: &str, samples: usize, sample: usize) -> PathBuf {
    if samples == 1 {
        out_dir.join(format!("{case_id}.jsonl"))
    } else {
        out_dir.join(format!("{case_id}.sample-{sample:03}.jsonl"))
    }
}

struct CaseRunContext<'a> {
    case: &'a EvalCase,
    base_dir: &'a Path,
    case_rel: &'a Path,
    case_dir: &'a Path,
    trace_path: &'a Path,
    provider: &'a dyn Provider,
    model: &'a str,
    gate_enabled: bool,
}

struct IsolatedWorkspace {
    root: PathBuf,
    base_dir: PathBuf,
    cleanup: IsolationCleanup,
}

enum IsolationCleanup {
    RemoveDir,
    GitWorktree { repo_root: PathBuf },
}

async fn run_one_case_isolated(ctx: CaseRunContext<'_>) -> anyhow::Result<VerifyResult> {
    let workspace = IsolatedWorkspace::create(ctx.base_dir, ctx.case_rel, ctx.case.reset.as_ref())
        .await
        .map_err(|e| anyhow::anyhow!("isolation setup failed: {e}"))?;
    let isolated_case_dir = workspace.base_dir.join(ctx.case_rel);
    let mut verdict = run_one_case_with_reset(CaseRunContext {
        case: ctx.case,
        base_dir: &workspace.base_dir,
        case_rel: ctx.case_rel,
        case_dir: &isolated_case_dir,
        trace_path: ctx.trace_path,
        provider: ctx.provider,
        model: ctx.model,
        gate_enabled: ctx.gate_enabled,
    })
    .await?;

    if let Err(e) = workspace.cleanup().await {
        verdict.passed = false;
        verdict.detail = format!("isolation cleanup failed: {e}\n{}", verdict.detail);
    }

    Ok(verdict)
}

impl IsolatedWorkspace {
    async fn create(
        base_dir: &Path,
        case_rel: &Path,
        reset: Option<&EvalReset>,
    ) -> anyhow::Result<Self> {
        if matches!(reset, Some(EvalReset::Git)) {
            return Self::create_git_worktree(base_dir).await;
        }
        Self::create_copy(base_dir, case_rel)
    }

    async fn create_git_worktree(base_dir: &Path) -> anyhow::Result<Self> {
        let base_dir = base_dir.canonicalize()?;
        let repo_root = git_output(&base_dir, ["rev-parse", "--show-toplevel"]).await?;
        let repo_root = PathBuf::from(repo_root.trim()).canonicalize()?;
        let _ = git_output(&repo_root, ["rev-parse", "--verify", "HEAD"]).await?;
        let root = unique_temp_dir("git-worktree");
        let _ = std::fs::remove_dir_all(&root);
        let root_arg = root.to_string_lossy().to_string();
        run_command(
            &repo_root,
            Path::new("."),
            "git",
            ["worktree", "add", "--detach", root_arg.as_str(), "HEAD"],
        )
        .await?;
        let base_rel = base_dir.strip_prefix(&repo_root).map_err(|e| {
            anyhow::anyhow!(
                "suite base {} is not inside git repo {}: {e}",
                base_dir.display(),
                repo_root.display()
            )
        })?;
        Ok(Self {
            base_dir: root.join(base_rel),
            root,
            cleanup: IsolationCleanup::GitWorktree { repo_root },
        })
    }

    fn create_copy(base_dir: &Path, case_rel: &Path) -> anyhow::Result<Self> {
        let base_dir = base_dir.canonicalize()?;
        let source_case_dir = base_dir.join(case_rel).canonicalize()?;
        if !source_case_dir.starts_with(&base_dir) {
            anyhow::bail!(
                "case dir {} escapes suite base {}",
                source_case_dir.display(),
                base_dir.display()
            );
        }
        let root = unique_temp_dir("copy");
        let _ = std::fs::remove_dir_all(&root);
        if case_rel == Path::new(".") {
            copy_dir_contents(&base_dir, &root)?;
        } else {
            let target_case_dir = root.join(case_rel);
            if let Some(parent) = target_case_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            copy_dir_contents(&source_case_dir, &target_case_dir)?;
        }
        Ok(Self {
            root: root.clone(),
            base_dir: root,
            cleanup: IsolationCleanup::RemoveDir,
        })
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        match self.cleanup {
            IsolationCleanup::RemoveDir => {
                std::fs::remove_dir_all(&self.root)?;
                Ok(())
            }
            IsolationCleanup::GitWorktree { repo_root } => {
                let root_arg = self.root.to_string_lossy().to_string();
                let result = run_command(
                    &repo_root,
                    Path::new("."),
                    "git",
                    ["worktree", "remove", "--force", root_arg.as_str()],
                )
                .await;
                if result.is_err() {
                    let _ = std::fs::remove_dir_all(&self.root);
                }
                result
            }
        }
    }
}

async fn git_output<I, S>(cwd: &Path, args: I) -> anyhow::Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect::<Vec<_>>();
    let output = CommandRunner::new(cwd).run("git", &args).await?;
    if !output.status.success() {
        anyhow::bail!(
            "`git {}` exited {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            args.join(" "),
            output.status,
            output.stdout,
            output.stderr
        );
    }
    Ok(output.stdout)
}

fn unique_temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("argus-eval-{tag}-{}-{nanos}", std::process::id()))
}

fn copy_dir_contents(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(
            name.to_string_lossy().as_ref(),
            ".git" | ".argus" | "target" | "node_modules"
        ) {
            continue;
        }
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            copy_dir_contents(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = dst_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

async fn run_one_case_with_reset(ctx: CaseRunContext<'_>) -> anyhow::Result<VerifyResult> {
    if let Some(reset) = &ctx.case.reset {
        reset_case(reset, ctx.base_dir, ctx.case_rel)
            .await
            .map_err(|e| anyhow::anyhow!("pre-reset failed: {e}"))?;
    }

    let mut verdict = run_one_case(
        ctx.case,
        ctx.case_dir,
        ctx.trace_path,
        ctx.provider,
        ctx.model,
        ctx.gate_enabled,
    )
    .await?;

    if let Some(reset) = &ctx.case.reset {
        if let Err(e) = reset_case(reset, ctx.base_dir, ctx.case_rel).await {
            verdict.passed = false;
            verdict.detail = format!("post-reset failed: {e}\n{}", verdict.detail);
        }
    }

    Ok(verdict)
}

async fn reset_case(reset: &EvalReset, base_dir: &Path, case_rel: &Path) -> anyhow::Result<()> {
    match reset {
        EvalReset::Git => {
            let case_rel = case_rel.to_string_lossy().to_string();
            run_command(
                base_dir,
                Path::new("."),
                "git",
                ["checkout", "--", &case_rel],
            )
            .await?;
            run_command(
                base_dir,
                Path::new("."),
                "git",
                ["clean", "-fd", "--", &case_rel],
            )
            .await?;
            Ok(())
        }
        EvalReset::Command { command } => {
            CommandRunner::in_workspace(base_dir, case_rel, ExecutionLimits::default())
                .run_shell(command)
                .await
                .map(|_| ())
        }
    }
}

async fn run_command<I, S>(
    workspace_root: &Path,
    cwd: &Path,
    program: &str,
    args: I,
) -> anyhow::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect::<Vec<_>>();
    let output = CommandRunner::in_workspace(workspace_root, cwd, ExecutionLimits::default())
        .run(program, &args)
        .await?;
    if output.status.success() {
        return Ok(());
    }
    anyhow::bail!(
        "`{} {}` exited {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        program,
        args.join(" "),
        output.status,
        output.stdout,
        output.stderr
    )
}

/// 跑单个 case:agent(带验证护栏自我修复)→ 独立裁决 verify。裁决结果也落 trace。
async fn run_one_case(
    case: &EvalCase,
    case_dir: &Path,
    trace_path: &Path,
    provider: &dyn Provider,
    model: &str,
    gate_enabled: bool,
) -> anyhow::Result<VerifyResult> {
    let mut trace = TraceWriter::create(trace_path)?;
    {
        let mut agent = Agent::new(provider, model, &mut trace)
            .with_tools(vec![
                Box::new(ReadFile::new(case_dir)),
                Box::new(WriteFile::new(case_dir)),
                Box::new(RunShell::new(case_dir)),
            ])
            .with_approver(Box::new(AutoApprover))
            .with_policy(SandboxPolicy::trusted());
        if gate_enabled {
            agent = agent.with_verifier(Box::new(CommandVerifier::new(
                case_dir,
                case.verify.clone(),
            )));
        }
        // agent 自身带验证护栏会尝试修复;这里忽略其返回文本,裁决以独立 verify 为准。
        let _ = agent.run(&case.task).await?;
    } // agent drop,释放对 trace 的可变借用

    // 独立裁决:再 verify 一次作为 source of truth,并把结果落进该 case 的 trace。
    let verdict = CommandVerifier::new(case_dir, case.verify.clone())
        .verify()
        .await;
    trace.record(EventKind::VerificationGate {
        passed: verdict.passed,
        detail: verdict.detail.clone(),
    })?;
    Ok(verdict)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_suite_json_with_defaults() {
        let json = r#"{"name":"s","cases":[{"id":"a","task":"do","verify":["true"]}]}"#;
        let suite: EvalSuite = serde_json::from_str(json).unwrap();
        assert_eq!(suite.name, "s");
        assert_eq!(suite.cases.len(), 1);
        assert_eq!(suite.cases[0].id, "a");
        assert_eq!(suite.cases[0].dir, None); // serde default
        assert_eq!(suite.cases[0].verify, vec!["true".to_string()]);
    }

    #[test]
    fn parses_git_reset_mode() {
        let json =
            r#"{"name":"s","cases":[{"id":"a","task":"do","reset":"git","verify":["true"]}]}"#;
        let suite: EvalSuite = serde_json::from_str(json).unwrap();
        assert!(matches!(suite.cases[0].reset, Some(EvalReset::Git)));
    }

    #[test]
    fn parses_command_reset_mode() {
        let json = r#"{"name":"s","cases":[{"id":"a","task":"do","reset":{"command":"touch reset.marker"},"verify":["true"]}]}"#;
        let suite: EvalSuite = serde_json::from_str(json).unwrap();
        assert!(matches!(
            &suite.cases[0].reset,
            Some(EvalReset::Command { command }) if command == "touch reset.marker"
        ));
    }

    #[test]
    fn report_counts_and_rate() {
        let report = SuiteReport {
            suite_name: "s".into(),
            results: vec![
                CaseResult {
                    id: "a".into(),
                    passed: true,
                    detail: "ok".into(),
                    trace_path: "a.jsonl".into(),
                },
                CaseResult {
                    id: "b".into(),
                    passed: false,
                    detail: "no".into(),
                    trace_path: "b.jsonl".into(),
                },
            ],
            attempts: Vec::new(),
            samples: 1,
            gate_enabled: true,
            warnings: Vec::new(),
        };
        assert_eq!(report.total(), 2);
        assert_eq!(report.passed_count(), 1);
        assert!(!report.all_passed());
        assert!((report.pass_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn empty_report_rate_is_zero() {
        let report = SuiteReport {
            suite_name: "s".into(),
            results: vec![],
            attempts: Vec::new(),
            samples: 1,
            gate_enabled: true,
            warnings: Vec::new(),
        };
        assert_eq!(report.pass_rate(), 0.0);
        assert!(report.all_passed()); // 空集 all() == true
    }

    #[tokio::test]
    async fn run_suite_reports_pass_and_fail_and_writes_traces() {
        use crate::provider::MockProvider;

        let base = std::env::temp_dir().join(format!("argus-eval-base-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let out = base.join("out");

        let suite = EvalSuite {
            name: "t".into(),
            cases: vec![
                EvalCase {
                    id: "ok".into(),
                    task: "do it".into(),
                    dir: None,
                    verify: vec!["true".into()],
                    reset: None,
                },
                EvalCase {
                    id: "bad".into(),
                    task: "do it".into(),
                    dir: None,
                    verify: vec!["false".into()],
                    reset: None,
                },
            ],
        };
        let provider = MockProvider::new();
        let report = run_suite(&suite, &base, &provider, "m", &out)
            .await
            .unwrap();

        assert_eq!(report.total(), 2);
        assert_eq!(report.passed_count(), 1);
        let ok = report.results.iter().find(|r| r.id == "ok").unwrap();
        let bad = report.results.iter().find(|r| r.id == "bad").unwrap();
        assert!(ok.passed, "ok detail: {}", ok.detail);
        assert!(!bad.passed, "bad detail: {}", bad.detail);

        // 每个 case 落了独立且非空的 trace
        assert!(out.join("ok.jsonl").exists(), "ok trace missing");
        assert!(out.join("bad.jsonl").exists(), "bad trace missing");
        assert!(std::fs::metadata(out.join("ok.jsonl")).unwrap().len() > 0);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn command_reset_runs_before_and_after_case() {
        use crate::provider::MockProvider;

        let base =
            std::env::temp_dir().join(format!("argus-eval-reset-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("state.txt"), "dirty").unwrap();
        let out = base.join("out");
        let suite = EvalSuite {
            name: "reset".into(),
            cases: vec![EvalCase {
                id: "case".into(),
                task: "write".into(),
                dir: None,
                verify: vec!["test \"$(cat state.txt)\" = clean".into()],
                reset: Some(EvalReset::Command {
                    command: "n=$(cat reset.count 2>/dev/null || echo 0); n=$((n + 1)); printf \"$n\" > reset.count; printf clean > state.txt".into(),
                }),
            }],
        };

        let report = run_suite_with_options(
            &suite,
            &base,
            &MockProvider::new(),
            "mock",
            &out,
            &EvalRunOptions {
                samples: 1,
                gate_enabled: true,
                isolation: EvalIsolation::InPlace,
            },
        )
        .await
        .unwrap();

        assert!(report.all_passed(), "report: {:?}", report.results);
        assert_eq!(
            std::fs::read_to_string(base.join("state.txt")).unwrap(),
            "clean"
        );
        assert_eq!(
            std::fs::read_to_string(base.join("reset.count")).unwrap(),
            "2"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn command_reset_works_with_relative_base_dir() {
        use crate::provider::MockProvider;

        let base_rel = PathBuf::from(format!("target/argus-eval-relative-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base_rel);
        std::fs::create_dir_all(base_rel.join("case")).unwrap();
        std::fs::write(base_rel.join("case").join("state.txt"), "dirty").unwrap();
        let suite = EvalSuite {
            name: "relative".into(),
            cases: vec![EvalCase {
                id: "case".into(),
                task: "write".into(),
                dir: Some("case".into()),
                verify: vec!["test \"$(cat state.txt)\" = clean".into()],
                reset: Some(EvalReset::Command {
                    command: "printf clean > state.txt".into(),
                }),
            }],
        };

        let report = run_suite_with_options(
            &suite,
            &base_rel,
            &MockProvider::new(),
            "mock",
            &base_rel.join("out"),
            &EvalRunOptions {
                samples: 1,
                gate_enabled: true,
                isolation: EvalIsolation::InPlace,
            },
        )
        .await
        .unwrap();

        assert!(report.all_passed(), "report: {:?}", report.results);
        assert_eq!(
            std::fs::read_to_string(base_rel.join("case").join("state.txt")).unwrap(),
            "clean"
        );
        let _ = std::fs::remove_dir_all(&base_rel);
    }

    #[tokio::test]
    async fn default_eval_isolation_keeps_reset_out_of_original_case_dir() {
        use crate::provider::MockProvider;

        let base = std::env::temp_dir().join(format!(
            "argus-eval-isolated-default-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("case")).unwrap();
        std::fs::write(base.join("case").join("state.txt"), "dirty").unwrap();
        let suite = EvalSuite {
            name: "isolated".into(),
            cases: vec![EvalCase {
                id: "case".into(),
                task: "make clean".into(),
                dir: Some("case".into()),
                verify: vec!["test \"$(cat state.txt)\" = clean".into()],
                reset: Some(EvalReset::Command {
                    command: "printf clean > state.txt".into(),
                }),
            }],
        };

        let report = run_suite_with_options(
            &suite,
            &base,
            &MockProvider::new(),
            "mock",
            &base.join("out"),
            &EvalRunOptions::default(),
        )
        .await
        .unwrap();

        assert!(report.all_passed(), "report: {report:?}");
        assert_eq!(
            std::fs::read_to_string(base.join("case").join("state.txt")).unwrap(),
            "dirty"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn eval_in_place_preserves_existing_reset_mutation_behavior() {
        use crate::provider::MockProvider;

        let base = std::env::temp_dir().join(format!("argus-eval-in-place-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("case")).unwrap();
        std::fs::write(base.join("case").join("state.txt"), "dirty").unwrap();
        let suite = EvalSuite {
            name: "in-place".into(),
            cases: vec![EvalCase {
                id: "case".into(),
                task: "make clean".into(),
                dir: Some("case".into()),
                verify: vec!["test \"$(cat state.txt)\" = clean".into()],
                reset: Some(EvalReset::Command {
                    command: "printf clean > state.txt".into(),
                }),
            }],
        };

        let report = run_suite_with_options(
            &suite,
            &base,
            &MockProvider::new(),
            "mock",
            &base.join("out"),
            &EvalRunOptions {
                samples: 1,
                gate_enabled: true,
                isolation: EvalIsolation::InPlace,
            },
        )
        .await
        .unwrap();

        assert!(report.all_passed(), "report: {report:?}");
        assert_eq!(
            std::fs::read_to_string(base.join("case").join("state.txt")).unwrap(),
            "clean"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn git_reset_uses_isolated_worktree_by_default() {
        use crate::provider::MockProvider;

        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let base =
            std::env::temp_dir().join(format!("argus-eval-reset-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("case")).unwrap();
        std::fs::write(base.join("case").join("state.txt"), "clean").unwrap();
        let init = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&base)
            .output()
            .unwrap();
        assert!(init.status.success(), "git init failed: {init:?}");
        let add = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&base)
            .output()
            .unwrap();
        assert!(add.status.success(), "git add failed: {add:?}");
        let commit = std::process::Command::new("git")
            .args([
                "-c",
                "user.email=a@b.c",
                "-c",
                "user.name=Argus",
                "commit",
                "-m",
                "init",
            ])
            .current_dir(&base)
            .output()
            .unwrap();
        assert!(commit.status.success(), "git commit failed: {commit:?}");
        std::fs::write(base.join("case").join("state.txt"), "dirty").unwrap();

        let suite = EvalSuite {
            name: "reset".into(),
            cases: vec![EvalCase {
                id: "case".into(),
                task: "write".into(),
                dir: Some("case".into()),
                verify: vec!["test \"$(cat state.txt)\" = clean".into()],
                reset: Some(EvalReset::Git),
            }],
        };

        let report = run_suite(
            &suite,
            &base,
            &MockProvider::new(),
            "mock",
            &base.join("out"),
        )
        .await
        .unwrap();

        assert!(report.all_passed(), "report: {:?}", report.results);
        assert_eq!(
            std::fs::read_to_string(base.join("case").join("state.txt")).unwrap(),
            "dirty"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    struct FixAfterVerificationProvider;

    #[async_trait::async_trait]
    impl Provider for FixAfterVerificationProvider {
        fn name(&self) -> &str {
            "fix-after-verification"
        }

        async fn complete(
            &self,
            req: &crate::types::CompletionRequest,
        ) -> anyhow::Result<crate::types::CompletionResponse> {
            let has_tool_result = req.messages.iter().any(|m| {
                m.content
                    .iter()
                    .any(|c| matches!(c, crate::types::Content::ToolResult { .. }))
            });
            let saw_verify_failure = req
                .messages
                .iter()
                .any(|m| m.text().contains("Verification failed"));
            let usage = crate::types::Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
            };
            if saw_verify_failure && !has_tool_result {
                return Ok(crate::types::CompletionResponse {
                    text: String::new(),
                    tool_calls: vec![crate::types::ToolCall {
                        id: "fix-1".into(),
                        name: "write_file".into(),
                        input: serde_json::json!({"path": "state.txt", "content": "clean"}),
                    }],
                    usage,
                    stop_reason: crate::types::StopReason::ToolUse,
                });
            }
            Ok(crate::types::CompletionResponse {
                text: "done".into(),
                tool_calls: vec![],
                usage,
                stop_reason: crate::types::StopReason::EndTurn,
            })
        }
    }

    #[tokio::test]
    async fn no_gate_disables_agent_self_repair_but_keeps_final_verify() {
        let base = std::env::temp_dir().join(format!("argus-eval-no-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let suite = EvalSuite {
            name: "gate".into(),
            cases: vec![EvalCase {
                id: "case".into(),
                task: "make state clean".into(),
                dir: None,
                verify: vec!["test \"$(cat state.txt 2>/dev/null)\" = clean".into()],
                reset: Some(EvalReset::Command {
                    command: "printf dirty > state.txt".into(),
                }),
            }],
        };

        let gated = run_suite_with_options(
            &suite,
            &base,
            &FixAfterVerificationProvider,
            "mock",
            &base.join("gated"),
            &EvalRunOptions::default(),
        )
        .await
        .unwrap();
        assert!(gated.all_passed(), "gated report: {gated:?}");

        let no_gate = run_suite_with_options(
            &suite,
            &base,
            &FixAfterVerificationProvider,
            "mock",
            &base.join("no-gate"),
            &EvalRunOptions {
                samples: 1,
                gate_enabled: false,
                isolation: EvalIsolation::Isolated,
            },
        )
        .await
        .unwrap();
        assert!(
            !no_gate.all_passed(),
            "no-gate should still fail final verify: {no_gate:?}"
        );
        assert_eq!(no_gate.attempts_total(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn repeated_sampling_writes_distinct_attempt_traces() {
        use crate::provider::MockProvider;

        let base = std::env::temp_dir().join(format!("argus-eval-samples-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let out = base.join("out");
        let suite = EvalSuite {
            name: "samples".into(),
            cases: vec![EvalCase {
                id: "case".into(),
                task: "do".into(),
                dir: None,
                verify: vec!["true".into()],
                reset: Some(EvalReset::Command {
                    command: "n=$(cat reset.count 2>/dev/null || echo 0); n=$((n + 1)); printf \"$n\" > reset.count".into(),
                }),
            }],
        };

        let report = run_suite_with_options(
            &suite,
            &base,
            &MockProvider::new(),
            "mock",
            &out,
            &EvalRunOptions {
                samples: 3,
                gate_enabled: true,
                isolation: EvalIsolation::InPlace,
            },
        )
        .await
        .unwrap();

        assert_eq!(report.attempts_total(), 3);
        assert_eq!(report.attempts_passed(), 3);
        assert!(out.join("case.sample-001.jsonl").exists());
        assert!(out.join("case.sample-002.jsonl").exists());
        assert!(out.join("case.sample-003.jsonl").exists());
        assert_eq!(
            std::fs::read_to_string(base.join("reset.count")).unwrap(),
            "6"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}

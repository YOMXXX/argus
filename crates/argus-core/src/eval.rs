//! Eval 引擎:在仓库上批量量化 agent 的通过率/回归。
//!
//! 一个 case = task + 一组 verify 命令;pass 判据 = 跑完 agent 后 verify 全部 exit 0。

use crate::agent::Agent;
use crate::approver::AutoApprover;
use crate::provider::Provider;
use crate::tool::{ReadFile, RunShell, WriteFile};
use crate::verifier::{CommandVerifier, Verifier, VerifyResult};
use argus_trace::{EventKind, TraceWriter};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
}

/// 单个 case 的运行结果。
#[derive(Debug, Clone)]
pub struct CaseResult {
    pub id: String,
    pub passed: bool,
    pub detail: String,
    pub trace_path: PathBuf,
}

/// 整个套件的报告。
#[derive(Debug, Clone)]
pub struct SuiteReport {
    pub suite_name: String,
    pub results: Vec<CaseResult>,
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
    std::fs::create_dir_all(out_dir)?;
    let mut results = Vec::with_capacity(suite.cases.len());
    for case in &suite.cases {
        let case_dir = base_dir.join(case.dir.as_deref().unwrap_or("."));
        let trace_path = out_dir.join(format!("{}.jsonl", case.id));
        let case_result = match run_one_case(case, &case_dir, &trace_path, provider, model).await {
            Ok(vr) => CaseResult {
                id: case.id.clone(),
                passed: vr.passed,
                detail: vr.detail,
                trace_path,
            },
            Err(e) => CaseResult {
                id: case.id.clone(),
                passed: false,
                detail: format!("case error: {e}"),
                trace_path,
            },
        };
        results.push(case_result);
    }
    Ok(SuiteReport {
        suite_name: suite.name.clone(),
        results,
    })
}

/// 跑单个 case:agent(带验证护栏自我修复)→ 独立裁决 verify。裁决结果也落 trace。
async fn run_one_case(
    case: &EvalCase,
    case_dir: &Path,
    trace_path: &Path,
    provider: &dyn Provider,
    model: &str,
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
            .with_verifier(Box::new(CommandVerifier::new(
                case_dir,
                case.verify.clone(),
            )));
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
                },
                EvalCase {
                    id: "bad".into(),
                    task: "do it".into(),
                    dir: None,
                    verify: vec!["false".into()],
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
}

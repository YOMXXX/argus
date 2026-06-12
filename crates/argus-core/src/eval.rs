//! Eval 引擎:在仓库上批量量化 agent 的通过率/回归。
//!
//! 一个 case = task + 一组 verify 命令;pass 判据 = 跑完 agent 后 verify 全部 exit 0。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
                CaseResult { id: "a".into(), passed: true, detail: "ok".into(), trace_path: "a.jsonl".into() },
                CaseResult { id: "b".into(), passed: false, detail: "no".into(), trace_path: "b.jsonl".into() },
            ],
        };
        assert_eq!(report.total(), 2);
        assert_eq!(report.passed_count(), 1);
        assert!(!report.all_passed());
        assert!((report.pass_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn empty_report_rate_is_zero() {
        let report = SuiteReport { suite_name: "s".into(), results: vec![] };
        assert_eq!(report.pass_rate(), 0.0);
        assert!(report.all_passed()); // 空集 all() == true
    }
}

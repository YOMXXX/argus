//! 省钱路由:便宜模型先干,验证护栏失败则升级强模型重跑。

use crate::agent::Agent;
use crate::approver::AutoApprover;
use crate::cost::estimate_cost;
use crate::provider::Provider;
use crate::tool::{ReadFile, RunShell, WriteFile};
use crate::verifier::{CommandVerifier, VerifyResult, Verifier};
use argus_trace::{read_trace, EventKind, TraceWriter};
use std::path::Path;

/// 一次路由运行的结果。
#[derive(Debug, Clone)]
pub struct RouteReport {
    pub cheap_model: String,
    pub strong_model: String,
    pub escalated: bool,
    pub passed: bool,
    pub final_text: String,
    pub cheap_cost: f64,
    pub strong_cost: f64,
    /// 反事实:假设全部 token 都按强模型单价跑的估算成本。
    pub always_strong_cost: f64,
}

impl RouteReport {
    pub fn actual_cost(&self) -> f64 {
        self.cheap_cost + self.strong_cost
    }
}

/// 跑单个 model 一遍:Agent(工具 + AutoApprover + 验证护栏)→ 返回 (最终文本, 独立裁决)。
/// 用块作用域释放 agent 对 `&mut trace` 的借用,以便调用方继续写 trace。
async fn run_one(
    provider: &dyn Provider,
    model: &str,
    work_dir: &Path,
    verify_cmds: &[String],
    task: &str,
    trace: &mut TraceWriter,
) -> anyhow::Result<(String, VerifyResult)> {
    let text;
    {
        let mut agent = Agent::new(provider, model, trace)
            .with_tools(vec![
                Box::new(ReadFile::new(work_dir)),
                Box::new(WriteFile::new(work_dir)),
                Box::new(RunShell::new(work_dir)),
            ])
            .with_approver(Box::new(AutoApprover))
            .with_verifier(Box::new(CommandVerifier::new(work_dir, verify_cmds.to_vec())));
        text = agent.run(task).await?;
    }
    // 独立裁决,作为是否升级的判据(Agent::run 熔断后仍返 Ok,故须独立判断)。
    let verdict = CommandVerifier::new(work_dir, verify_cmds.to_vec()).verify().await;
    Ok((text, verdict))
}

/// 累加 trace 中指定 model 的 ModelResponse token(prompt, completion)。
fn sum_tokens(events: &[argus_trace::TraceEvent], model: &str) -> (u64, u64) {
    let mut p = 0u64;
    let mut c = 0u64;
    for e in events {
        if let EventKind::ModelResponse { model: m, prompt_tokens, completion_tokens, .. } = &e.kind {
            if m == model {
                p += prompt_tokens;
                c += completion_tokens;
            }
        }
    }
    (p, c)
}

/// escalation 路由:cheap 先跑,验证失败则升级 strong 重跑;返回结果与成本估算。
///
/// 前置:`verify_cmds` 非空(路由靠验证护栏判断是否升级)。
pub async fn run_with_escalation(
    provider: &dyn Provider,
    cheap_model: &str,
    strong_model: &str,
    work_dir: &Path,
    trace_path: &Path,
    verify_cmds: &[String],
    task: &str,
) -> anyhow::Result<RouteReport> {
    if let Some(parent) = trace_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut trace = TraceWriter::create(trace_path)?;

    // 第一档:便宜模型
    let (cheap_text, cheap_verdict) =
        run_one(provider, cheap_model, work_dir, verify_cmds, task, &mut trace).await?;

    let (escalated, passed, final_text) = if cheap_verdict.passed || strong_model == cheap_model {
        (false, cheap_verdict.passed, cheap_text)
    } else {
        // 升级:记录决策,再用强模型重跑
        trace.record(EventKind::RouteDecision {
            from_model: cheap_model.to_string(),
            to_model: strong_model.to_string(),
            reason: format!("cheap model verification failed: {}", cheap_verdict.detail),
        })?;
        let (strong_text, strong_verdict) =
            run_one(provider, strong_model, work_dir, verify_cmds, task, &mut trace).await?;
        (true, strong_verdict.passed, strong_text)
    };

    // 读 trace 按 model 累加 token,折算成本
    drop(trace); // flush
    let events = read_trace(trace_path)?;
    let (cheap_p, cheap_c) = sum_tokens(&events, cheap_model);
    let (strong_p, strong_c) = if escalated { sum_tokens(&events, strong_model) } else { (0, 0) };
    let cheap_cost = estimate_cost(cheap_model, cheap_p, cheap_c);
    let strong_cost = estimate_cost(strong_model, strong_p, strong_c);
    let always_strong_cost = estimate_cost(strong_model, cheap_p + strong_p, cheap_c + strong_c);

    Ok(RouteReport {
        cheap_model: cheap_model.to_string(),
        strong_model: strong_model.to_string(),
        escalated,
        passed,
        final_text,
        cheap_cost,
        strong_cost,
        always_strong_cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MockProvider;

    #[tokio::test]
    async fn escalates_when_cheap_fails() {
        // verify "false" → cheap 必失败 → 升级 strong → 仍失败但要记录升级
        let dir = std::env::temp_dir().join(format!("argus-route-esc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let trace = dir.join("t.jsonl");
        let provider = MockProvider::new();

        let report = run_with_escalation(
            &provider, "claude-3-5-haiku-latest", "claude-sonnet-4-5",
            &dir, &trace, &["false".to_string()], "do it",
        ).await.unwrap();

        assert!(report.escalated, "should escalate when cheap fails");
        assert!(!report.passed, "verify=false never passes");
        // trace 里应有 RouteDecision 事件
        let events = read_trace(&trace).unwrap();
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::RouteDecision { .. })),
            "trace should record the escalation");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn no_escalation_when_cheap_passes() {
        // verify "true" → cheap 通过 → 不升级
        let dir = std::env::temp_dir().join(format!("argus-route-pass-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let trace = dir.join("t.jsonl");
        let provider = MockProvider::new();

        let report = run_with_escalation(
            &provider, "claude-3-5-haiku-latest", "claude-sonnet-4-5",
            &dir, &trace, &["true".to_string()], "do it",
        ).await.unwrap();

        assert!(!report.escalated, "should not escalate when cheap passes");
        assert!(report.passed);
        assert_eq!(report.strong_cost, 0.0, "strong not used → zero strong cost");
        let events = read_trace(&trace).unwrap();
        assert!(!events.iter().any(|e| matches!(&e.kind, EventKind::RouteDecision { .. })),
            "no escalation event when cheap passes");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

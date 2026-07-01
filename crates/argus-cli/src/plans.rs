use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static PLAN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanRecord {
    pub id: String,
    pub goal: String,
    pub status: String,
    pub steps: Vec<PlanStep>,
    pub created_ms: u128,
    pub updated_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub id: String,
    pub text: String,
    pub status: String,
    pub evidence: Vec<String>,
}

pub fn create_plan(root: &Path, goal: &str) -> Result<PlanRecord> {
    let goal = normalize(goal);
    if goal == "(none)" {
        anyhow::bail!("plan goal must not be empty");
    }
    let now = now_ms();
    let plan = PlanRecord {
        id: next_plan_id(now),
        goal: goal.clone(),
        status: "active".into(),
        steps: default_steps(&goal),
        created_ms: now,
        updated_ms: now,
    };
    write_plan(root, &plan)?;
    Ok(plan)
}

pub fn load_current_plan(root: &Path) -> Result<Option<PlanRecord>> {
    let path = current_plan_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&text).map_err(|e| {
        anyhow::anyhow!("invalid current plan {}: {e}", path.display())
    })?))
}

pub fn queue_next_step(root: &Path) -> Result<Option<crate::tasks::TaskRecord>> {
    let Some(mut plan) = load_current_plan(root)? else {
        return Ok(None);
    };
    let Some(index) = plan.steps.iter().position(|step| step.status == "pending") else {
        return Ok(None);
    };
    let text = format!("Plan {}: {}", plan.goal, plan.steps[index].text);
    let task = crate::tasks::queue_task(root, &text)?;
    plan.steps[index].status = "queued".into();
    plan.updated_ms = now_ms();
    write_plan(root, &plan)?;
    Ok(Some(task))
}

pub fn complete_current_step(root: &Path, evidence: &str) -> Result<PlanRecord> {
    let Some(mut plan) = load_current_plan(root)? else {
        anyhow::bail!("no active plan found");
    };
    let Some(index) = plan.steps.iter().position(|step| step.status != "done") else {
        return Ok(plan);
    };
    plan.steps[index].status = "done".into();
    let evidence = normalize(evidence);
    if evidence != "(none)" {
        plan.steps[index].evidence.push(evidence);
    }
    if plan.steps.iter().all(|step| step.status == "done") {
        plan.status = "done".into();
    }
    plan.updated_ms = now_ms();
    write_plan(root, &plan)?;
    Ok(plan)
}

pub fn load_plan_status(root: &Path) -> Result<String> {
    Ok(render_plan_status(load_current_plan(root)?.as_ref()))
}

pub fn render_plan_status(plan: Option<&PlanRecord>) -> String {
    let mut lines = Vec::new();
    lines.push("Planning Engine".to_string());
    let Some(plan) = plan else {
        lines.push("(no active plan)".into());
        lines.push("Next: /plan <goal>".into());
        return lines.join("\n");
    };
    lines.push(format!("Goal: {}", plan.goal));
    lines.push(format!("Status: {}", plan.status));
    for step in plan.steps.iter().take(5) {
        lines.push(format!(
            "[{}] {} {}",
            step.status,
            step.id,
            compact(&step.text, 88)
        ));
        if let Some(evidence) = step.evidence.last() {
            lines.push(format!("evidence: {}", compact(evidence, 84)));
        }
    }
    if plan.status == "done" {
        lines.push("Next: review and start a new /plan".into());
    } else {
        lines.push("Next: /next or /done <evidence>".into());
    }
    lines.join("\n")
}

pub fn current_plan_path(root: &Path) -> PathBuf {
    root.join(".argus/plans/current.json")
}

fn write_plan(root: &Path, plan: &PlanRecord) -> Result<()> {
    let path = current_plan_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(plan)?)?;
    Ok(())
}

fn default_steps(goal: &str) -> Vec<PlanStep> {
    [
        format!("Clarify scope and acceptance gates for: {goal}"),
        format!("Implement the smallest high-value slice for: {goal}"),
        format!("Verify, review, and document: {goal}"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, text)| PlanStep {
        id: format!("step-{}", index + 1),
        text,
        status: "pending".into(),
        evidence: Vec::new(),
    })
    .collect()
}

fn normalize(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "(none)".into()
    } else {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

fn compact(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn next_plan_id(created_ms: u128) -> String {
    let sequence = PLAN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("plan-{created_ms}-{}-{sequence}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("argus-plan-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn plan_next_and_done_roundtrip() {
        let dir = temp_dir("roundtrip");
        std::fs::create_dir_all(&dir).unwrap();

        let plan = create_plan(&dir, "ship focused planning").unwrap();
        assert_eq!(plan.goal, "ship focused planning");
        assert_eq!(plan.status, "active");
        assert_eq!(plan.steps.len(), 3);
        assert!(current_plan_path(&dir).exists());

        let task = queue_next_step(&dir).unwrap().unwrap();
        assert!(task.text.contains("ship focused planning"), "{task:?}");
        let queued = load_current_plan(&dir).unwrap().unwrap();
        assert_eq!(queued.steps[0].status, "queued");

        let completed = complete_current_step(&dir, "cargo test passed").unwrap();
        assert_eq!(completed.steps[0].status, "done");
        assert_eq!(completed.steps[0].evidence, vec!["cargo test passed"]);
        assert_eq!(completed.status, "active");

        let rendered = render_plan_status(Some(&completed));
        assert!(rendered.contains("Planning Engine"), "{rendered}");
        assert!(rendered.contains("ship focused planning"), "{rendered}");
        assert!(rendered.contains("[done]"), "{rendered}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_plan_status_prompts_for_plan() {
        let rendered = render_plan_status(None);

        assert!(rendered.contains("(no active plan)"), "{rendered}");
        assert!(rendered.contains("/plan <goal>"), "{rendered}");
    }
}

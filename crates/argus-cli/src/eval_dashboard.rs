use anyhow::Result;
use serde_json::Value;
use std::path::Path;

const EVALS_DIR: &str = ".argus/evals";

pub fn load_eval_dashboard(root: &Path) -> Result<String> {
    let dir = root.join(EVALS_DIR);
    if !dir.exists() {
        return Ok("Eval Dashboard\n(no eval suites found)".into());
    }

    let mut suites = std::fs::read_dir(&dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    suites.sort();

    if suites.is_empty() {
        return Ok("Eval Dashboard\n(no eval suites found)".into());
    }

    let mut lines = vec![
        "Eval Dashboard".to_string(),
        format!("Suites: {}", suites.len()),
    ];
    for path in suites {
        lines.push(String::new());
        lines.extend(render_suite(&path));
    }
    Ok(lines.join("\n"))
}

fn render_suite(path: &Path) -> Vec<String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("(unknown)");
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => return vec![format!("{file_name}  unreadable: {err}")],
    };
    let value = match serde_json::from_str::<Value>(&text) {
        Ok(value) => value,
        Err(err) => return vec![format!("{file_name}  invalid json: {err}")],
    };

    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(file_name);
    let cases = value
        .get("cases")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut lines = vec![format!("{file_name}  {name}  {} case(s)", cases.len())];
    for case in cases.iter().take(5) {
        let id = case.get("id").and_then(Value::as_str).unwrap_or("(no id)");
        let verify_count = case
            .get("verify")
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0);
        lines.push(format!("- {id}  verify: {verify_count} command(s)"));
    }
    if cases.len() > 5 {
        lines.push(format!("... {} more case(s)", cases.len() - 5));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "arguscode-eval-dashboard-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn load_eval_dashboard_summarizes_eval_suites() {
        let dir = temp_dir("summary");
        std::fs::create_dir_all(dir.join(EVALS_DIR)).unwrap();
        std::fs::write(
            dir.join(EVALS_DIR).join("smoke.json"),
            r#"{"name":"demo smoke","cases":[{"id":"smoke","task":"check","verify":["cargo test"]}]}"#,
        )
        .unwrap();

        let dashboard = load_eval_dashboard(&dir).unwrap();

        assert!(dashboard.contains("Eval Dashboard"), "{dashboard}");
        assert!(dashboard.contains("Suites: 1"), "{dashboard}");
        assert!(dashboard.contains("demo smoke"), "{dashboard}");
        assert!(dashboard.contains("smoke"), "{dashboard}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

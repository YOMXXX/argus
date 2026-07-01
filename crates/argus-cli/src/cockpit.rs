use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const COCKPIT_EVENTS_PATH: &str = ".argus/cockpit/events.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CockpitEvent {
    pub phase: String,
    pub detail: String,
    pub next: String,
    pub created_ms: u128,
}

pub fn cockpit_events_path(root: &Path) -> PathBuf {
    root.join(COCKPIT_EVENTS_PATH)
}

pub fn append_cockpit_event(
    root: &Path,
    phase: &str,
    detail: &str,
    next: &str,
) -> Result<CockpitEvent> {
    let record = CockpitEvent {
        phase: normalize(phase),
        detail: normalize(detail),
        next: normalize(next),
        created_ms: now_ms(),
    };
    let path = cockpit_events_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(&record)?;
    line.push('\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    Ok(record)
}

pub fn list_cockpit_events(root: &Path) -> Result<Vec<CockpitEvent>> {
    let path = cockpit_events_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)?;
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: CockpitEvent = serde_json::from_str(line).map_err(|e| {
            anyhow::anyhow!(
                "invalid cockpit event line {} in {}: {e}",
                index + 1,
                path.display()
            )
        })?;
        records.push(record);
    }
    Ok(records)
}

pub fn load_cockpit_journal(root: &Path) -> Result<String> {
    Ok(render_cockpit_journal(&list_cockpit_events(root)?))
}

pub fn render_cockpit_journal(events: &[CockpitEvent]) -> String {
    let mut lines = Vec::new();
    lines.push("Execution Cockpit".to_string());
    if events.is_empty() {
        lines.push("(no execution events yet)".into());
        lines.push("Next: type a task or run /verify".into());
        return lines.join("\n");
    }
    for event in events.iter().rev().take(6).rev() {
        lines.push(format!(
            "[{}] {}",
            compact(&event.phase, 18),
            compact(&event.detail, 88)
        ));
        if event.next != "(none)" {
            lines.push(format!("next: {}", compact(&event.next, 92)));
        }
    }
    lines.join("\n")
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
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
            "argus-cockpit-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn cockpit_events_append_and_render_recent_journal() {
        let dir = temp_dir("journal");
        std::fs::create_dir_all(&dir).unwrap();

        append_cockpit_event(&dir, "queue", "queued task task-1", "/run").unwrap();
        append_cockpit_event(&dir, "verify", "1 check(s) passed", "/review").unwrap();

        let events = list_cockpit_events(&dir).unwrap();
        let journal = render_cockpit_journal(&events);

        assert_eq!(events.len(), 2);
        assert!(journal.contains("Execution Cockpit"), "{journal}");
        assert!(journal.contains("[queue] queued task task-1"), "{journal}");
        assert!(journal.contains("next: /review"), "{journal}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_cockpit_journal_has_next_action() {
        let journal = render_cockpit_journal(&[]);

        assert!(journal.contains("(no execution events yet)"), "{journal}");
        assert!(
            journal.contains("Next: type a task or run /verify"),
            "{journal}"
        );
    }
}

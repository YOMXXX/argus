#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureKind {
    Configuration,
    Timeout,
    Command,
    Unknown,
}

pub fn classify_failure(detail: &str) -> FailureKind {
    let detail = detail.to_ascii_lowercase();
    if detail.contains("no verify command") || detail.contains("not configured") {
        FailureKind::Configuration
    } else if detail.contains("timed out") || detail.contains("timeout") {
        FailureKind::Timeout
    } else if detail.contains("exit status")
        || detail.contains("command failed")
        || detail.contains("verification failed")
    {
        FailureKind::Command
    } else {
        FailureKind::Unknown
    }
}

pub fn build_repair_task(commands: &[String], detail: &str) -> String {
    let kind = match classify_failure(detail) {
        FailureKind::Configuration => "configuration",
        FailureKind::Timeout => "timeout",
        FailureKind::Command => "command",
        FailureKind::Unknown => "unknown",
    };
    let commands = if commands.is_empty() {
        "(none configured)".to_string()
    } else {
        commands.join(" && ")
    };
    format!(
        "Repair verification failure ({kind}). Commands: {commands}. Failure: {}",
        compact(detail, 320)
    )
}

fn compact(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut out = normalized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_failure_detects_common_verify_failures() {
        assert_eq!(
            classify_failure("no verify command configured"),
            FailureKind::Configuration
        );
        assert_eq!(classify_failure("command timed out"), FailureKind::Timeout);
        assert_eq!(
            classify_failure("command failed with exit status 1"),
            FailureKind::Command
        );
    }

    #[test]
    fn build_repair_task_includes_commands_and_failure_detail() {
        let task = build_repair_task(
            &["cargo test --workspace --locked".into()],
            "command failed with exit status 1",
        );

        assert!(task.contains("Repair verification failure"), "{task}");
        assert!(task.contains("cargo test --workspace --locked"), "{task}");
        assert!(task.contains("command failed with exit status 1"), "{task}");
    }
}

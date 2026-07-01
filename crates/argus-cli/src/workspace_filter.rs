pub fn reviewable_status_lines(status: &str) -> impl Iterator<Item = &str> {
    status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !is_argus_runtime_status_line(line))
}

pub fn is_argus_runtime_status_line(line: &str) -> bool {
    [
        ".argus/tasks/",
        ".argus/sessions/",
        ".argus/reviews/",
        ".argus/cockpit/",
        ".argus/checkpoints/",
        ".argus/eval/",
        ".argus/eval-runs/",
    ]
    .iter()
    .any(|runtime_path| line.contains(runtime_path))
}

#[cfg(test)]
mod tests {
    #[test]
    fn reviewable_status_lines_filters_argus_runtime_metadata() {
        let status = "?? src/lib.rs\n?? .argus/cockpit/events.jsonl\n M README.md\n";

        let lines = super::reviewable_status_lines(status).collect::<Vec<_>>();

        assert_eq!(lines, vec!["?? src/lib.rs", " M README.md"]);
    }
}

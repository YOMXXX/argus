use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
}

pub fn load_launch_checklist(root: &Path) -> Result<Vec<LaunchCheck>> {
    Ok(vec![
        file_check(root, "CI workflow", ".github/workflows/ci.yml"),
        file_check(root, "Release workflow", ".github/workflows/release.yml"),
        file_check(root, "README", "README.md"),
        file_check(root, "Changelog", "CHANGELOG.md"),
        file_check(root, "Installer", "install.sh"),
        file_check(root, "Release packager", "scripts/package-release.sh"),
        any_file_check(
            root,
            "Demo material",
            &[
                "benchmarks/demo.gif",
                "benchmarks/demo.tape",
                "launch/demo-script.md",
            ],
        ),
        benchmark_result_check(root),
    ])
}

pub fn render_launch_checklist(checks: &[LaunchCheck]) -> String {
    let ready = checks
        .iter()
        .filter(|check| check.status == "ready")
        .count();
    let mut lines = vec![
        "Launch Readiness".to_string(),
        format!("Ready: {ready}/{}", checks.len()),
    ];
    for check in checks {
        lines.push(format!(
            "[{}] {} - {}",
            check.status, check.name, check.detail
        ));
    }
    lines.join("\n")
}

fn file_check(root: &Path, name: &str, rel: &str) -> LaunchCheck {
    let path = root.join(rel);
    if path.is_file() {
        ready(name, rel)
    } else {
        missing(name, rel)
    }
}

fn any_file_check(root: &Path, name: &str, rels: &[&str]) -> LaunchCheck {
    for rel in rels {
        if root.join(rel).is_file() {
            return ready(name, rel);
        }
    }
    missing(name, &rels.join(" or "))
}

fn benchmark_result_check(root: &Path) -> LaunchCheck {
    if root.join("benchmarks/reliability.json").is_file() {
        return ready("Benchmark result", "benchmarks/reliability.json");
    }
    let results_dir = root.join("benchmarks/results");
    if let Ok(entries) = std::fs::read_dir(&results_dir) {
        let has_result = entries
            .filter_map(Result::ok)
            .any(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("md"));
        if has_result {
            return ready("Benchmark result", "benchmarks/results/*.md");
        }
    }
    missing(
        "Benchmark result",
        "benchmarks/reliability.json or benchmarks/results/*.md",
    )
}

fn ready(name: &str, detail: &str) -> LaunchCheck {
    LaunchCheck {
        name: name.into(),
        status: "ready".into(),
        detail: detail.into(),
    }
}

fn missing(name: &str, detail: &str) -> LaunchCheck {
    LaunchCheck {
        name: name.into(),
        status: "missing".into(),
        detail: detail.into(),
    }
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
            "argus-launch-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn launch_checklist_reports_ready_and_missing_signals() {
        let dir = temp_dir("signals");
        std::fs::create_dir_all(dir.join(".github/workflows")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join(".github/workflows/ci.yml"), "name: CI\n").unwrap();
        std::fs::write(dir.join("README.md"), "# Demo\n").unwrap();
        std::fs::write(dir.join("CHANGELOG.md"), "# Changelog\n").unwrap();
        std::fs::write(dir.join("install.sh"), "#!/bin/sh\n").unwrap();
        std::fs::write(dir.join("scripts/package-release.sh"), "#!/bin/sh\n").unwrap();

        let checks = load_launch_checklist(&dir).unwrap();
        let rendered = render_launch_checklist(&checks);

        assert!(rendered.contains("Launch Readiness"), "{rendered}");
        assert!(rendered.contains("[ready] CI workflow"), "{rendered}");
        assert!(rendered.contains("[missing] Demo material"), "{rendered}");
        assert!(
            rendered.contains("[missing] Benchmark result"),
            "{rendered}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

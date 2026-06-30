#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("argus-cli should live under crates/argus-cli")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("argus-{name}-{}-{nanos}", std::process::id()))
}

fn host_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        _ => None,
    }
}

fn write_fake_bins(dir: &Path) -> (PathBuf, PathBuf) {
    std::fs::create_dir_all(dir).unwrap();
    let argus = dir.join("argus");
    let arguscode = dir.join("arguscode");
    std::fs::write(&argus, "#!/bin/sh\necho argus 0.1.1-test\n").unwrap();
    std::fs::write(&arguscode, "#!/bin/sh\necho arguscode 0.1.1-test\n").unwrap();
    for bin in [&argus, &arguscode] {
        let mut perms = std::fs::metadata(bin).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(bin, perms).unwrap();
    }
    (argus, arguscode)
}

fn sha256(path: &Path) -> String {
    let output = Command::new("sh")
        .arg("-c")
        .arg("if command -v sha256sum >/dev/null 2>&1; then sha256sum \"$1\"; else shasum -a 256 \"$1\"; fi")
        .arg("sh")
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "checksum command failed: {output:?}"
    );
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn release_version_script_accepts_matching_tag_and_rejects_mismatch() {
    let root = repo_root();
    let ok = Command::new("sh")
        .arg(root.join("scripts/check-release-version.sh"))
        .arg("v0.1.1")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(ok.status.success(), "matching tag should pass: {ok:?}");

    let bad = Command::new("sh")
        .arg(root.join("scripts/check-release-version.sh"))
        .arg("v9.9.9")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(!bad.status.success(), "mismatched tag should fail");
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("does not match workspace version"),
        "stderr: {stderr}"
    );
}

#[test]
fn package_release_script_writes_archive_and_checksum() {
    let Some(target) = host_target() else {
        return;
    };
    let root = repo_root();
    let base = temp_dir("package-release");
    let (fake_argus, fake_arguscode) = write_fake_bins(&base.join("bin"));
    let dist = base.join("dist");

    let out = Command::new("sh")
        .arg(root.join("scripts/package-release.sh"))
        .arg(target)
        .env("ARGUS_BIN_PATH", &fake_argus)
        .env("ARGUSCODE_BIN_PATH", &fake_arguscode)
        .env("ARGUS_DIST_DIR", &dist)
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(out.status.success(), "package script failed: {out:?}");

    let archive = dist.join(format!("argus-{target}.tar.gz"));
    let checksum = dist.join(format!("argus-{target}.tar.gz.sha256"));
    assert!(archive.exists(), "missing archive {}", archive.display());
    assert!(checksum.exists(), "missing checksum {}", checksum.display());
    let checksum_text = std::fs::read_to_string(checksum).unwrap();
    assert!(
        checksum_text.contains(&format!("argus-{target}.tar.gz")),
        "checksum should name archive: {checksum_text}"
    );
    let listing = Command::new("tar")
        .args(["-tzf"])
        .arg(&archive)
        .output()
        .unwrap();
    assert!(listing.status.success(), "tar listing failed: {listing:?}");
    let listing = String::from_utf8_lossy(&listing.stdout);
    assert!(
        listing.contains("argus"),
        "archive should include binary: {listing}"
    );
    assert!(
        listing.contains("arguscode"),
        "archive should include ArgusCode binary: {listing}"
    );
    assert!(
        listing.contains("README.md"),
        "archive should include README: {listing}"
    );
    assert!(
        listing.contains("LICENSE-MIT"),
        "archive should include MIT license: {listing}"
    );
    assert!(
        listing.contains("LICENSE-APACHE"),
        "archive should include Apache license: {listing}"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn installer_installs_local_release_and_verifies_checksum() {
    let Some(target) = host_target() else {
        return;
    };
    let root = repo_root();
    let base = temp_dir("install-release");
    let release = base.join("release");
    let staging = base.join("staging");
    let dest = base.join("bin");
    std::fs::create_dir_all(&release).unwrap();
    write_fake_bins(&staging);

    let archive = release.join(format!("argus-{target}.tar.gz"));
    let tar = Command::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .arg("argus")
        .arg("arguscode")
        .output()
        .unwrap();
    assert!(tar.status.success(), "tar failed: {tar:?}");
    let sum = sha256(&archive);
    std::fs::write(
        release.join(format!("argus-{target}.tar.gz.sha256")),
        format!("{sum}  argus-{target}.tar.gz\n"),
    )
    .unwrap();

    let out = Command::new("sh")
        .arg(root.join("install.sh"))
        .env(
            "ARGUS_RELEASE_BASE_URL",
            format!("file://{}", release.display()),
        )
        .env("ARGUS_INSTALL_DIR", &dest)
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(out.status.success(), "installer failed: {out:?}");

    let installed = dest.join("argus");
    let installed_code = dest.join("arguscode");
    assert!(installed.exists(), "missing installed argus");
    assert!(installed_code.exists(), "missing installed arguscode");
    let version = Command::new(&installed).arg("--version").output().unwrap();
    assert!(
        version.status.success(),
        "installed binary failed: {version:?}"
    );
    let stdout = String::from_utf8_lossy(&version.stdout);
    assert!(stdout.contains("argus 0.1.1-test"), "stdout: {stdout}");
    let code_version = Command::new(&installed_code)
        .arg("--version")
        .output()
        .unwrap();
    assert!(
        code_version.status.success(),
        "installed arguscode failed: {code_version:?}"
    );
    let stdout = String::from_utf8_lossy(&code_version.stdout);
    assert!(stdout.contains("arguscode 0.1.1-test"), "stdout: {stdout}");

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn release_workflow_uses_version_guard_packaging_script_and_checksums() {
    let workflow = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("release workflow should exist");

    assert!(
        workflow.contains("scripts/check-release-version.sh"),
        "release workflow should guard tag/version consistency"
    );
    assert!(
        workflow.contains("scripts/package-release.sh"),
        "release workflow should use the local packaging script"
    );
    assert!(
        workflow.contains("*.sha256"),
        "release workflow should upload checksum files"
    );
    assert!(
        workflow.contains("actions/upload-artifact"),
        "matrix builds should upload artifacts instead of publishing partial releases"
    );
    assert!(
        workflow.contains("actions/download-artifact"),
        "a final publish job should collect all artifacts before release upload"
    );
    assert!(
        workflow.contains("body_path: CHANGELOG.md"),
        "release should include release notes instead of publishing a blank body"
    );
    assert!(
        workflow.contains("cargo test --workspace --locked"),
        "release validation should run locked workspace tests"
    );
}

#[test]
fn workspace_path_dependencies_are_versioned_for_crates_io() {
    let cargo = std::fs::read_to_string(repo_root().join("Cargo.toml")).unwrap();

    assert!(
        cargo.contains(r#"argus-trace = { path = "crates/argus-trace", version = "0.1.1" }"#),
        "argus-trace workspace dependency needs a version for publishing"
    );
    assert!(
        cargo.contains(r#"argus-core = { path = "crates/argus-core", version = "0.1.1" }"#),
        "argus-core workspace dependency needs a version for publishing"
    );
}

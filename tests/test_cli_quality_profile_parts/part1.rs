use std::fs;
use std::process::Command;

fn init_git_repo(path: &std::path::Path) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("init")
        .arg("-q")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.email")
        .arg("anchor-test@example.invalid")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.name")
        .arg("Anchor Test")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("add")
        .arg("-A")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("commit")
        .arg("-q")
        .arg("-m")
        .arg("base")
        .status()
        .unwrap()
        .success());
}

#[test]
fn cli_quality_profile_flags_unverified_edit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.rs");
    fs::write(
        &source,
        "pub fn login() -> bool {\n    true\n}\n\npub fn logout() -> bool {\n    false\n}\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");

    let build = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let edit = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("login")
        .arg("--content")
        .arg("pub fn login() -> bool {\n    false\n}")
        .output()
        .unwrap();
    assert!(
        edit.status.success(),
        "edit failed: {}\n{}",
        String::from_utf8_lossy(&edit.stderr),
        String::from_utf8_lossy(&edit.stdout)
    );

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(
        receipt.status.success(),
        "receipt failed: {}",
        String::from_utf8_lossy(&receipt.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    assert_eq!(json["quality"]["score"], 25);
    assert_eq!(json["quality"]["risk"], "high");
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(flags
        .iter()
        .any(|flag| flag == "changed_without_recorded_context"));
    assert!(flags
        .iter()
        .any(|flag| flag == "changed_without_recorded_check"));
    assert!(flags
        .iter()
        .any(|flag| flag == "edited_file_without_prior_context"));
    let recommendations = json["quality"]["recommendations"].as_array().unwrap();
    assert!(recommendations.iter().any(|item| {
        item.as_str()
            .unwrap_or_default()
            .contains("anchor context/task")
    }));
    assert_eq!(json["summary"]["edits_without_file_context"], 1);
    assert_eq!(json["summary"]["unresolved_edits_without_file_context"], 1);
    assert_eq!(json["summary"]["max_changed_lines"], 1);
}

#[test]
fn cli_quality_profile_resolves_context_miss_after_reread() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("src.rs"),
        "pub fn login() -> bool {\n    true\n}\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");

    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("login")
        .arg("--content")
        .arg("pub fn login() -> bool {\n    false\n}")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("login")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(receipt.status.success());
    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(!flags
        .iter()
        .any(|flag| flag == "edited_file_without_prior_context"));
    assert_eq!(json["summary"]["edits_without_file_context"], 1);
    assert_eq!(json["summary"]["unresolved_edits_without_file_context"], 0);
}

#[test]
fn cli_quality_profile_flags_risky_path_without_check() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("auth.rs");
    fs::write(&source, "pub fn login() -> bool {\n    true\n}\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");

    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("login")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("auth.rs")
        .arg("--symbol")
        .arg("login")
        .arg("--content")
        .arg("pub fn login() -> bool {\n    false\n}")
        .status()
        .unwrap()
        .success());

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(receipt.status.success());

    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(flags
        .iter()
        .any(|flag| flag == "changed_without_recorded_check"));
    assert!(flags
        .iter()
        .any(|flag| flag == "risky_path_changed_without_check"));
    assert_eq!(json["summary"]["risky_paths"][0], "auth.rs");
}


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

#[test]
fn cli_quality_profile_flags_oversized_edit_scope() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.rs");
    fs::write(
        &source,
        "pub fn render() -> usize {\n    1\n}\n\npub fn stable() -> usize {\n    2\n}\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let large_body = format!(
        "pub fn render() -> usize {{\n{}\n    1\n}}",
        (0..160)
            .map(|i| format!("    let _v{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n")
    );

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
        .arg("render")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("render")
        .arg("--content")
        .arg(large_body)
        .status()
        .unwrap()
        .success());
    let weak_check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("test -f src.rs")
        .output()
        .unwrap();
    assert!(
        !weak_check.status.success(),
        "weak check should fail handoff\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&weak_check.stdout),
        String::from_utf8_lossy(&weak_check.stderr)
    );

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(receipt.status.success());

    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(flags.iter().any(|flag| flag == "oversized_edit_scope"));
    assert_eq!(json["summary"]["oversized_edits"], 1);
    assert!(json["summary"]["max_changed_lines"].as_u64().unwrap() > 150);
}

#[test]
fn cli_quality_profile_flags_raw_repo_change_without_anchor_write_event() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.rs");
    fs::write(&source, "pub fn value() -> bool {\n    true\n}\n").unwrap();
    init_git_repo(dir.path());

    let anchor = env!("CARGO_BIN_EXE_anchor");

    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .status()
        .unwrap()
        .success());

    fs::write(&source, "pub fn value() -> bool {\n    false\n}\n").unwrap();

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
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(flags.iter().any(|flag| flag == "unrecorded_repo_changes"));
    assert_eq!(json["summary"]["unrecorded_changed_files"], 1);
    assert_eq!(json["summary"]["unrecorded_changed_file_list"][0], "src.rs");
}

#[test]
fn cli_quality_profile_does_not_flag_anchor_recorded_write_as_raw_change() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.rs");
    fs::write(&source, "pub fn value() -> bool {\n    true\n}\n").unwrap();
    init_git_repo(dir.path());

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
        .arg("value")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("value")
        .arg("--content")
        .arg("pub fn value() -> bool {\n    false\n}")
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
    assert!(!flags.iter().any(|flag| flag == "unrecorded_repo_changes"));
    assert_eq!(json["summary"]["unrecorded_changed_files"], 0);
}

#[test]
fn cli_quality_profile_does_not_count_context_paths_as_broad_scope() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..5 {
        fs::write(
            dir.path().join(format!("src{i}.rs")),
            format!("pub fn value{i}() -> bool {{\n    true\n}}\n"),
        )
        .unwrap();
    }

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
        .arg("value0")
        .arg("value1")
        .arg("value2")
        .arg("value3")
        .arg("value4")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src0.rs")
        .arg("--symbol")
        .arg("value0")
        .arg("--content")
        .arg("pub fn value0() -> bool {\n    false\n}")
        .status()
        .unwrap()
        .success());
    let weak_check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("test -f src0.rs")
        .output()
        .unwrap();
    assert!(
        !weak_check.status.success(),
        "weak check should fail handoff\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&weak_check.stdout),
        String::from_utf8_lossy(&weak_check.stderr)
    );

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(receipt.status.success());

    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(
        !flags.iter().any(|flag| flag == "broad_file_scope"),
        "context reads should not count as broad edit scope: {}",
        String::from_utf8_lossy(&receipt.stdout)
    );
    assert!(flags
        .iter()
        .any(|flag| flag == "changed_without_test_check"));
    assert_eq!(json["quality"]["score"], 85);
    assert_eq!(json["summary"]["paths"].as_array().unwrap().len(), 5);
    assert_eq!(json["summary"]["changed_file_scope"], 1);
    assert_eq!(
        json["summary"]["changed_file_scope_paths"][0],
        serde_json::Value::String("src0.rs".to_string())
    );
    assert_eq!(
        json["summary"]["recorded_write_paths"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn cli_check_warns_when_changed_code_has_no_test_check() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("src.rs"),
        "pub fn value() -> bool {\n    true\n}\n",
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
        .arg("context")
        .arg("value")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("value")
        .arg("--content")
        .arg("pub fn value() -> bool {\n    false\n}")
        .status()
        .unwrap()
        .success());

    let check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("true")
        .output()
        .unwrap();
    assert!(
        !check.status.success(),
        "weak check should fail handoff\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("<kind>non_test</kind>"), "{stdout}");
    assert!(stdout.contains("<quality_feedback>"), "{stdout}");
    assert!(stdout.contains("test-like Anchor check"), "{stdout}");
    assert!(
        stdout.contains("<handoff_gate status=\"blocked\" reason=\"missing_test_check\"/>"),
        "{stdout}"
    );

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
        .any(|flag| flag == "changed_without_test_check"));
    assert_eq!(json["summary"]["test_checks_ok"], 0);
    assert_eq!(json["summary"]["checks_ok"], 1);
}

#[test]
fn cli_quality_profile_tracks_unresolved_failed_checks() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("src.rs"),
        "pub fn value() -> bool {\n    true\n}\n",
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
        .arg("context")
        .arg("value")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("value")
        .arg("--content")
        .arg("pub fn value() -> bool {\n    false\n}")
        .status()
        .unwrap()
        .success());

    let failed_check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("test -f pass.flag")
        .output()
        .unwrap();
    assert!(!failed_check.status.success());
    let failed_stdout = String::from_utf8_lossy(&failed_check.stdout);
    assert!(
        failed_stdout
            .contains("<handoff_gate status=\"blocked\" reason=\"unresolved_failed_check\"/>"),
        "{failed_stdout}"
    );

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(receipt.status.success());
    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(flags.iter().any(|flag| flag == "unresolved_failed_check"));
    assert_eq!(json["summary"]["checks_failed"], 1);
    assert_eq!(json["summary"]["unresolved_checks_failed"], 1);
    assert_eq!(json["summary"]["errors"], 0);

    fs::write(dir.path().join("pass.flag"), "").unwrap();
    let passing_check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("test -f pass.flag")
        .output()
        .unwrap();
    assert!(
        !passing_check.status.success(),
        "non-test retry should resolve the failed check but still block handoff"
    );
    let passing_stdout = String::from_utf8_lossy(&passing_check.stdout);
    assert!(
        passing_stdout.contains("<handoff_gate status=\"blocked\" reason=\"missing_test_check\"/>"),
        "{passing_stdout}"
    );

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(receipt.status.success());
    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(!flags.iter().any(|flag| flag == "unresolved_failed_check"));
    assert!(flags
        .iter()
        .any(|flag| flag == "changed_without_test_check"));
    assert_eq!(json["summary"]["checks_failed"], 1);
    assert_eq!(json["summary"]["unresolved_checks_failed"], 0);
    assert_eq!(json["summary"]["errors"], 0);
}

#[test]
fn cli_quality_profile_treats_retried_stale_write_as_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.rs");
    fs::write(&source, "pub fn value() -> bool {\n    true\n}\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");

    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .status()
        .unwrap()
        .success());
    let context = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("value")
        .arg("--limit")
        .arg("1")
        .output()
        .unwrap();
    assert!(context.status.success());
    let stdout = String::from_utf8_lossy(&context.stdout);
    let original_hash = stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("<file_hash>")
                .and_then(|line| line.strip_suffix("</file_hash>"))
        })
        .unwrap()
        .to_string();

    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("value")
        .arg("--content")
        .arg("pub fn value() -> bool {\n    maybe\n}")
        .status()
        .unwrap()
        .success());
    let stale_edit = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("value")
        .arg("--expect-hash")
        .arg(&original_hash)
        .arg("--content")
        .arg("pub fn value() -> bool {\n    false\n}")
        .output()
        .unwrap();
    assert!(!stale_edit.status.success());

    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("value")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("value")
        .arg("--content")
        .arg("pub fn value() -> bool {\n    false\n}")
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
    assert!(!flags.iter().any(|flag| flag == "stale_write_blocked"));
    assert_eq!(json["summary"]["stale_write_blocks"], 1);
    assert_eq!(json["summary"]["unresolved_stale_write_blocks"], 0);
    assert_eq!(json["summary"]["errors"], 0);
    assert_eq!(json["summary"]["unresolved_errors"], 0);
}

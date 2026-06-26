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


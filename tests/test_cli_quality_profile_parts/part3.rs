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


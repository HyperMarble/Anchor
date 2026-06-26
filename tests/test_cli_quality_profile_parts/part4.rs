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

use std::fs;
use std::process::Command;

#[test]
fn cli_edit_symbol_replaces_only_indexed_symbol_and_reindexes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.rs");
    fs::write(
        &source,
        "pub fn old_name() -> bool {\n    true\n}\n\npub fn stable() -> bool {\n    false\n}\n",
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
        .arg("old_name")
        .arg("--content")
        .arg("pub fn new_name() -> bool {\n    false\n}")
        .output()
        .unwrap();
    assert!(
        edit.status.success(),
        "symbol edit failed: {}\n{}",
        String::from_utf8_lossy(&edit.stderr),
        String::from_utf8_lossy(&edit.stdout)
    );

    let updated = fs::read_to_string(&source).unwrap();
    assert!(updated.contains("pub fn new_name() -> bool"));
    assert!(updated.contains("pub fn stable() -> bool"));
    assert!(!updated.contains("pub fn old_name() -> bool"));

    let raw_symbols = fs::read_to_string(dir.path().join(".anchor/index/symbols.json")).unwrap();
    let symbols: serde_json::Value = serde_json::from_str(&raw_symbols).unwrap();
    let names: Vec<&str> = symbols["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|symbol| symbol["name"].as_str())
        .collect();

    assert!(names.contains(&"new_name"), "{names:?}");
    assert!(names.contains(&"stable"), "{names:?}");
    assert!(!names.contains(&"old_name"), "{names:?}");
}

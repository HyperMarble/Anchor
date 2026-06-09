use std::fs;
use std::process::Command;

#[test]
fn cli_edit_reindexes_changed_file_without_rebuild() {
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
        .arg("--action")
        .arg("replace")
        .arg("--pattern")
        .arg("old_name")
        .arg("--content")
        .arg("new_name")
        .output()
        .unwrap();
    assert!(
        edit.status.success(),
        "edit failed: {}",
        String::from_utf8_lossy(&edit.stderr)
    );
    let edit_stdout = String::from_utf8_lossy(&edit.stdout);
    assert!(edit_stdout.contains("<before_hash>"), "{edit_stdout}");
    assert!(edit_stdout.contains("<after_hash>"), "{edit_stdout}");
    assert!(
        !edit_stdout.contains("<old>old_name</old>"),
        "{edit_stdout}"
    );
    assert!(
        !edit_stdout.contains("<new>new_name</new>"),
        "{edit_stdout}"
    );

    let new_hit = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("search")
        .arg("new_name")
        .arg("--limit")
        .arg("5")
        .output()
        .unwrap();
    assert!(
        new_hit.status.success(),
        "new search failed: {}",
        String::from_utf8_lossy(&new_hit.stderr)
    );
    let new_stdout = String::from_utf8_lossy(&new_hit.stdout);
    assert!(new_stdout.contains("new_name"), "{new_stdout}");

    let raw_symbols = fs::read_to_string(dir.path().join(".anchor/index/symbols.json")).unwrap();
    let symbols: serde_json::Value = serde_json::from_str(&raw_symbols).unwrap();
    let names: Vec<&str> = symbols["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|symbol| symbol["name"].as_str())
        .collect();

    assert!(names.contains(&"new_name"), "{names:?}");
    assert!(!names.contains(&"old_name"), "{names:?}");
}

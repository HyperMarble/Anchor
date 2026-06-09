use anchor::storage::content_hash;
use std::fs;
use std::process::Command;

fn hash_text(text: &str) -> String {
    content_hash(text.as_bytes())
}

#[test]
fn cli_edit_expect_hash_blocks_stale_file_without_mutating() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.py");
    fs::write(&source, "value = 1\n").unwrap();

    let stale_hash = hash_text("value = 0\n");
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let edit = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.py")
        .arg("--action")
        .arg("replace")
        .arg("--pattern")
        .arg("value = 1")
        .arg("--content")
        .arg("value = 2")
        .arg("--expect-hash")
        .arg(stale_hash)
        .output()
        .unwrap();

    assert!(
        !edit.status.success(),
        "stale edit should fail: stdout={} stderr={}",
        String::from_utf8_lossy(&edit.stdout),
        String::from_utf8_lossy(&edit.stderr)
    );
    let stdout = String::from_utf8_lossy(&edit.stdout);
    assert!(stdout.contains("<status>stale_file</status>"), "{stdout}");
    assert!(stdout.contains("<expected_hash>"), "{stdout}");
    assert!(stdout.contains("<actual_hash>"), "{stdout}");
    assert_eq!(fs::read_to_string(&source).unwrap(), "value = 1\n");
}

#[test]
fn cli_edit_expect_hash_allows_matching_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.py");
    fs::write(&source, "value = 1\n").unwrap();

    let current_hash = hash_text("value = 1\n");
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let edit = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.py")
        .arg("--action")
        .arg("replace")
        .arg("--pattern")
        .arg("value = 1")
        .arg("--content")
        .arg("value = 2")
        .arg("--expect-hash")
        .arg(current_hash)
        .output()
        .unwrap();

    assert!(
        edit.status.success(),
        "matching edit failed: stdout={} stderr={}",
        String::from_utf8_lossy(&edit.stdout),
        String::from_utf8_lossy(&edit.stderr)
    );
    let stdout = String::from_utf8_lossy(&edit.stdout);
    assert!(stdout.contains("<status>replaced</status>"), "{stdout}");
    assert!(stdout.contains("<before_hash>"), "{stdout}");
    assert!(stdout.contains("<after_hash>"), "{stdout}");
    assert!(
        stdout.contains(
            r#"<changed_range start="1" old_end="1" new_end="1" old_lines="1" new_lines="1"/>"#
        ),
        "{stdout}"
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), "value = 2\n");
}

#[test]
fn cli_write_expect_hash_missing_allows_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let write = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("write")
        .arg("new.py")
        .arg("print('ok')\n")
        .arg("--expect-hash")
        .arg("missing")
        .output()
        .unwrap();

    assert!(
        write.status.success(),
        "new file write failed: stdout={} stderr={}",
        String::from_utf8_lossy(&write.stdout),
        String::from_utf8_lossy(&write.stderr)
    );
    let stdout = String::from_utf8_lossy(&write.stdout);
    assert!(stdout.contains("<status>created</status>"), "{stdout}");
    assert!(stdout.contains("<after_hash>"), "{stdout}");
    assert!(
        stdout.contains(
            r#"<changed_range start="1" old_end="0" new_end="1" old_lines="0" new_lines="1"/>"#
        ),
        "{stdout}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("new.py")).unwrap(),
        "print('ok')\n"
    );
}

#[test]
fn cli_write_blocks_existing_source_file_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.py");
    fs::write(&source, "def value():\n    return 1\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let write = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("write")
        .arg("src.py")
        .arg("def value():\n    return 2\n")
        .output()
        .unwrap();

    assert!(
        !write.status.success(),
        "existing source write should fail: stdout={} stderr={}",
        String::from_utf8_lossy(&write.stdout),
        String::from_utf8_lossy(&write.stderr)
    );
    let stdout = String::from_utf8_lossy(&write.stdout);
    assert!(
        stdout.contains("<status>source_write_requires_edit</status>"),
        "{stdout}"
    );
    assert!(
        stdout.contains("existing source files must be changed through anchor edit"),
        "{stdout}"
    );
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "def value():\n    return 1\n"
    );

    let raw_events = fs::read_to_string(dir.path().join(".anchor/events/events.jsonl")).unwrap();
    assert!(raw_events.contains("\"event_type\":\"write.guard\""));
    assert!(raw_events.contains("\"status\":\"blocked\""));
}

#[test]
fn cli_edit_without_expect_hash_uses_last_context_read_hash() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.py");
    fs::write(&source, "def value():\n    return 1\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let agent_id = "write-guard-agent";
    let session_id = "write-guard-session";

    let context = Command::new(anchor)
        .env("ANCHOR_AGENT_ID", agent_id)
        .env("ANCHOR_SESSION_ID", session_id)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("value")
        .output()
        .unwrap();
    assert!(
        context.status.success(),
        "context failed: stdout={} stderr={}",
        String::from_utf8_lossy(&context.stdout),
        String::from_utf8_lossy(&context.stderr)
    );
    let context_stdout = String::from_utf8_lossy(&context.stdout);
    assert!(context_stdout.contains("<file_hash>"), "{context_stdout}");

    fs::write(&source, "def value():\n    return 9\n").unwrap();

    let edit = Command::new(anchor)
        .env("ANCHOR_AGENT_ID", agent_id)
        .env("ANCHOR_SESSION_ID", session_id)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.py")
        .arg("--action")
        .arg("replace")
        .arg("--pattern")
        .arg("return 9")
        .arg("--content")
        .arg("return 2")
        .output()
        .unwrap();

    assert!(
        !edit.status.success(),
        "automatic stale edit should fail: stdout={} stderr={}",
        String::from_utf8_lossy(&edit.stdout),
        String::from_utf8_lossy(&edit.stderr)
    );
    let stdout = String::from_utf8_lossy(&edit.stdout);
    assert!(stdout.contains("<status>stale_file</status>"), "{stdout}");
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "def value():\n    return 9\n"
    );
}

#[test]
fn cli_default_agent_id_is_stable_across_rooted_commands() {
    let dir = tempfile::tempdir().unwrap();
    let launch_dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.py");
    fs::write(&source, "def value():\n    return 1\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");

    let context = Command::new(anchor)
        .env_remove("ANCHOR_AGENT_ID")
        .env_remove("ANCHOR_SESSION_ID")
        .current_dir(launch_dir.path())
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("value")
        .output()
        .unwrap();
    assert!(
        context.status.success(),
        "context failed: stdout={} stderr={}",
        String::from_utf8_lossy(&context.stdout),
        String::from_utf8_lossy(&context.stderr)
    );

    fs::write(&source, "def value():\n    return 9\n").unwrap();

    let edit = Command::new(anchor)
        .env_remove("ANCHOR_AGENT_ID")
        .env_remove("ANCHOR_SESSION_ID")
        .current_dir(launch_dir.path())
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.py")
        .arg("--action")
        .arg("replace")
        .arg("--pattern")
        .arg("return 9")
        .arg("--content")
        .arg("return 2")
        .output()
        .unwrap();

    assert!(
        !edit.status.success(),
        "default-agent stale edit should fail: stdout={} stderr={}",
        String::from_utf8_lossy(&edit.stdout),
        String::from_utf8_lossy(&edit.stderr)
    );
    let stdout = String::from_utf8_lossy(&edit.stdout);
    assert!(stdout.contains("<status>stale_file</status>"), "{stdout}");

    let raw_events = fs::read_to_string(dir.path().join(".anchor/events/events.jsonl")).unwrap();
    let agent_ids: std::collections::BTreeSet<String> = raw_events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter_map(|event| event["agent_id"].as_str().map(str::to_string))
        .collect();
    assert_eq!(agent_ids.len(), 1, "{raw_events}");
    assert!(
        agent_ids.iter().next().unwrap().starts_with("anchor-"),
        "{raw_events}"
    );
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "def value():\n    return 9\n"
    );
}

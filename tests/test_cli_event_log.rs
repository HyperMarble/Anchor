use std::fs;
use std::process::Command;

#[test]
fn cli_symbol_edit_records_execution_event() {
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
        .env("ANCHOR_AGENT_ID", "agent-event-test")
        .env("ANCHOR_SESSION_ID", "session-event-test")
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

    let events_path = dir.path().join(".anchor/events/events.jsonl");
    let raw_events = fs::read_to_string(&events_path)
        .unwrap_or_else(|e| panic!("missing event log {}: {e}", events_path.display()));

    let events: Vec<serde_json::Value> = raw_events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    let edit_event = events
        .iter()
        .find(|event| {
            event["event_type"] == "edit.apply"
                && event["symbol"] == "old_name"
                && event["path"] == "src.rs"
        })
        .expect("missing edit.apply event");

    assert_eq!(edit_event["status"], "ok");
    assert_eq!(edit_event["agent_id"], "agent-event-test");
    assert_eq!(edit_event["session_id"], "session-event-test");
}

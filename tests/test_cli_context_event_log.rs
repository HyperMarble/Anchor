use std::fs;
use std::process::Command;

#[test]
fn cli_context_records_read_and_cache_events() {
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

    for _ in 0..2 {
        let context = Command::new(anchor)
            .env("ANCHOR_AGENT_ID", "agent-context-test")
            .env("ANCHOR_SESSION_ID", "session-context-test")
            .arg("--root")
            .arg(dir.path())
            .arg("context")
            .arg("login")
            .arg("--limit")
            .arg("1")
            .output()
            .unwrap();
        assert!(
            context.status.success(),
            "context failed: {}\n{}",
            String::from_utf8_lossy(&context.stderr),
            String::from_utf8_lossy(&context.stdout)
        );
    }

    let events_path = dir.path().join(".anchor/events/events.jsonl");
    let raw_events = fs::read_to_string(&events_path)
        .unwrap_or_else(|e| panic!("missing event log {}: {e}", events_path.display()));

    let events: Vec<serde_json::Value> = raw_events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    let read_statuses: Vec<&str> = events
        .iter()
        .filter(|event| {
            event["event_type"] == "context.read"
                && event["symbol"] == "login"
                && event["path"] == "src.rs"
        })
        .filter_map(|event| event["status"].as_str())
        .collect();

    assert!(
        read_statuses.contains(&"ok"),
        "missing ok context.read event: {raw_events}"
    );
    assert!(
        read_statuses.contains(&"cached"),
        "missing cached context.read event: {raw_events}"
    );

    let first_read = events
        .iter()
        .find(|event| event["event_type"] == "context.read" && event["status"] == "ok")
        .expect("missing context.read ok event");
    assert_eq!(first_read["agent_id"], "agent-context-test");
    assert_eq!(first_read["session_id"], "session-context-test");
}

use std::fs;
use std::process::Command;

#[test]
fn cli_check_records_verification_result() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("printf verified")
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}\n{}",
        String::from_utf8_lossy(&check.stderr),
        String::from_utf8_lossy(&check.stdout)
    );

    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("<status>ok</status>"), "{stdout}");
    assert!(stdout.contains("verified"), "{stdout}");

    let events_path = dir.path().join(".anchor/events/events.jsonl");
    let raw_events = fs::read_to_string(&events_path)
        .unwrap_or_else(|e| panic!("missing event log {}: {e}", events_path.display()));
    let events: Vec<serde_json::Value> = raw_events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert!(
        events.iter().any(|event| {
            event["event_type"] == "check.run"
                && event["status"] == "ok"
                && event["message"]
                    .as_str()
                    .map(|message| message.contains("cmd=sh -c printf verified"))
                    .unwrap_or(false)
        }),
        "missing check.run event: {raw_events}"
    );

    let status = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("status")
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "status failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        status_stdout.contains("<checks_ok>1</checks_ok>"),
        "{status_stdout}"
    );
    assert!(
        status_stdout.contains("<signal name=\"checks\" status=\"ok\" passed=\"1\" failed=\"0\"/>"),
        "{status_stdout}"
    );
}

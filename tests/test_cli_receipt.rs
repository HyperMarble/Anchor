use std::path::Path;
use std::process::Command;

#[path = "test_cli_support.rs"]
mod support;

#[test]
fn cli_receipt_exports_machine_readable_summary() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let mut check_cmd = Command::new(anchor);
    check_cmd
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--");
    support::print_text("receipt").apply(&mut check_cmd);
    let check = check_cmd.output().unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
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
    assert_eq!(json["schema"], "anchor.receipt.v1");
    assert_eq!(json["summary"]["event_count"], 1);
    assert_eq!(json["summary"]["checks_ok"], 1);
    assert_eq!(json["summary"]["checks_failed"], 0);
    assert_eq!(json["quality"]["score"], 100);
    assert_eq!(json["quality"]["risk"], "low");
    let event_log = Path::new(json["event_log"].as_str().unwrap());
    assert!(event_log.ends_with(Path::new(".anchor").join("events").join("events.jsonl")));
}

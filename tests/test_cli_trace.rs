use std::process::Command;

#[test]
fn cli_trace_prints_recent_execution_events() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("printf traced")
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    let trace = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("trace")
        .arg("--limit")
        .arg("1")
        .output()
        .unwrap();
    assert!(
        trace.status.success(),
        "trace failed: {}",
        String::from_utf8_lossy(&trace.stderr)
    );

    let stdout = String::from_utf8_lossy(&trace.stdout);
    assert!(
        stdout.contains("<trace count=\"1\" shown=\"1\">"),
        "{stdout}"
    );
    assert!(stdout.contains("type=\"check.run\""), "{stdout}");
    assert!(stdout.contains("status=\"ok\""), "{stdout}");
    assert!(
        stdout.contains("<message>exit=0 cmd=sh -c printf traced</message>"),
        "{stdout}"
    );
}

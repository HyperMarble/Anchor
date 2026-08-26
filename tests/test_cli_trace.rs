use std::process::Command;

#[path = "test_cli_support.rs"]
mod support;

#[test]
fn cli_trace_prints_recent_execution_events() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");
    let script = support::print_text("traced");

    let mut check_cmd = Command::new(anchor);
    check_cmd
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--");
    script.apply(&mut check_cmd);
    let check = check_cmd.output().unwrap();
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
        stdout.contains(&format!(
            "<message>exit=0 cmd={}</message>",
            script.recorded_text()
        )),
        "{stdout}"
    );
}

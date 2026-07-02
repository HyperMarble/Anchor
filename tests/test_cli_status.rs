use std::process::Command;

#[test]
fn cli_status_summarizes_recorded_checks() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("true")
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "check failed: {}\n{}",
        String::from_utf8_lossy(&check.stderr),
        String::from_utf8_lossy(&check.stdout)
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

    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("<events>1</events>"), "{stdout}");
    assert!(stdout.contains("<checks_ok>1</checks_ok>"), "{stdout}");
    assert!(
        stdout.contains("<signal name=\"checks\" status=\"ok\" passed=\"1\" failed=\"0\"/>"),
        "{stdout}"
    );
}

use std::fs;
use std::process::Command;

fn init_git_repo(path: &std::path::Path) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("init")
        .arg("-q")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.email")
        .arg("anchor-test@example.invalid")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.name")
        .arg("Anchor Test")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("add")
        .arg("-A")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("commit")
        .arg("-q")
        .arg("-m")
        .arg("base")
        .status()
        .unwrap()
        .success());
}

#[test]
fn cli_gate_passes_clean_execution_state() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let gate = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("gate")
        .arg("--min-score")
        .arg("85")
        .output()
        .unwrap();
    assert!(
        gate.status.success(),
        "gate should pass clean state\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&gate.stdout),
        String::from_utf8_lossy(&gate.stderr)
    );

    let stdout = String::from_utf8_lossy(&gate.stdout);
    assert!(stdout.contains("<score>100</score>"), "{stdout}");
    assert!(stdout.contains("<handoff_ready>true</handoff_ready>"), "{stdout}");
    assert!(stdout.contains("<status>ok</status>"), "{stdout}");
}

#[test]
fn cli_gate_blocks_unrecorded_repo_changes() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("src.py"), "value = 1\n").unwrap();
    init_git_repo(dir.path());
    fs::write(dir.path().join("src.py"), "value = 2\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let gate = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("gate")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&gate.stdout);
    assert!(!gate.status.success(), "gate must fail raw change: {stdout}");
    assert!(stdout.contains("<unreceipted_file>src.py</unreceipted_file>"), "{stdout}");
    assert!(stdout.contains("<status>failed</status>"), "{stdout}");
}

#[test]
fn cli_gate_blocks_unresolved_failed_check() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    let check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("exit 7")
        .output()
        .unwrap();
    assert!(!check.status.success());

    let gate = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("gate")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&gate.stdout);
    assert!(!gate.status.success(), "gate must fail failed check: {stdout}");
    assert!(
        stdout.contains("<handoff_blocker reason=\"unresolved_failed_check\">"),
        "{stdout}"
    );
    assert!(stdout.contains("<status>failed</status>"), "{stdout}");
}

use std::fs;
use std::process::Command;

#[test]
fn cli_gate_fails_unverified_edit_below_threshold() {
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

    let edit = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("login")
        .arg("--content")
        .arg("pub fn login() -> bool {\n    false\n}")
        .output()
        .unwrap();
    assert!(
        edit.status.success(),
        "edit failed: {}",
        String::from_utf8_lossy(&edit.stderr)
    );

    let gate = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("gate")
        .arg("--min-score")
        .arg("85")
        .output()
        .unwrap();
    assert!(
        !gate.status.success(),
        "gate should fail for unverified edit\nstdout:\n{}",
        String::from_utf8_lossy(&gate.stdout)
    );

    let stdout = String::from_utf8_lossy(&gate.stdout);
    assert!(stdout.contains("<score>25</score>"), "{stdout}");
    assert!(stdout.contains("<status>failed</status>"), "{stdout}");
    assert!(
        stdout.contains("<flag>changed_without_recorded_check</flag>"),
        "{stdout}"
    );
}

#[test]
fn cli_gate_blocks_non_test_check_after_source_edit() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.rs");
    fs::write(
        &source,
        "pub fn login() -> bool {\n    true\n}\n\npub fn logout() -> bool {\n    false\n}\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");

    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("login")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("login")
        .arg("--content")
        .arg("pub fn login() -> bool {\n    false\n}")
        .status()
        .unwrap()
        .success());
    let weak_check = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("test -f src.rs")
        .output()
        .unwrap();
    assert!(
        !weak_check.status.success(),
        "weak check should fail handoff\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&weak_check.stdout),
        String::from_utf8_lossy(&weak_check.stderr)
    );

    let gate = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("gate")
        .arg("--min-score")
        .arg("85")
        .output()
        .unwrap();
    assert!(
        !gate.status.success(),
        "gate should block edit verified only by a non-test check\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&gate.stdout),
        String::from_utf8_lossy(&gate.stderr)
    );

    let stdout = String::from_utf8_lossy(&gate.stdout);
    assert!(stdout.contains("<score>85</score>"), "{stdout}");
    assert!(
        stdout.contains("<flag>changed_without_test_check</flag>"),
        "{stdout}"
    );
    assert!(
        stdout.contains("<handoff_ready>false</handoff_ready>"),
        "{stdout}"
    );
    assert!(
        stdout.contains("<handoff_blocker reason=\"missing_test_check\">"),
        "{stdout}"
    );
    assert!(stdout.contains("<status>failed</status>"), "{stdout}");
}

#[test]
fn cli_gate_passes_after_test_like_check() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("src.rs"),
        "pub fn login() -> bool {\n    true\n}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("test_anchor_gate.py"),
        "import unittest\n\nclass SmokeTest(unittest.TestCase):\n    def test_passes(self):\n        self.assertTrue(True)\n",
    )
    .unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");

    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("login")
        .arg("--limit")
        .arg("1")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("login")
        .arg("--content")
        .arg("pub fn login() -> bool {\n    false\n}")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("python3")
        .arg("-m")
        .arg("unittest")
        .arg("discover")
        .status()
        .unwrap()
        .success());

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
        "gate should pass after a test-like check\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&gate.stdout),
        String::from_utf8_lossy(&gate.stderr)
    );

    let stdout = String::from_utf8_lossy(&gate.stdout);
    assert!(
        stdout.contains("<handoff_ready>true</handoff_ready>"),
        "{stdout}"
    );
    assert!(stdout.contains("<status>ok</status>"), "{stdout}");
}

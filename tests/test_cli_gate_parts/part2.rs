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

#[test]
fn cli_gate_flags_raw_write_but_not_anchor_edit() {
    let dir = tempfile::tempdir().unwrap();
    let anchor = env!("CARGO_BIN_EXE_anchor");
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/a.py"), "def f():\n    return 1\n").unwrap();
    assert!(Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(dir.path())
        .status()
        .unwrap()
        .success());
    Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();

    // raw write outside anchor -> require-receipts must fail and name the file
    std::fs::write(dir.path().join("src/a.py"), "def f():\n    return 2\n").unwrap();
    let raw = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("gate")
        .output()
        .unwrap();
    let raw_out = String::from_utf8_lossy(&raw.stdout);
    assert!(
        !raw.status.success(),
        "receipt gate must fail on raw write: {raw_out}"
    );
    assert!(raw_out.contains("<unreceipted_file>"), "{raw_out}");
    assert!(raw_out.contains("src/a.py"), "{raw_out}");

    // same change through anchor edit -> no unreceipted files
    std::fs::write(dir.path().join("src/a.py"), "def f():\n    return 1\n").unwrap();
    Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("f")
        .output()
        .unwrap();
    Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src/a.py")
        .arg("--action")
        .arg("replace")
        .arg("--pattern")
        .arg("return 1")
        .arg("--content")
        .arg("return 2")
        .output()
        .unwrap();
    let governed = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("gate")
        .output()
        .unwrap();
    let gov_out = String::from_utf8_lossy(&governed.stdout);
    assert!(
        !gov_out.contains("<unreceipted_file>"),
        "anchor-edited file must not be flagged as unreceipted: {gov_out}"
    );
}

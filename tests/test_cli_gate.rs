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
    assert!(stdout.contains("<score>45</score>"), "{stdout}");
    assert!(stdout.contains("<status>failed</status>"), "{stdout}");
    assert!(
        stdout.contains("<flag>changed_without_recorded_check</flag>"),
        "{stdout}"
    );
}

#[test]
fn cli_gate_passes_verified_context_edit() {
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
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("check")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("test -f src.rs")
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
        "gate should pass verified edit\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&gate.stdout),
        String::from_utf8_lossy(&gate.stderr)
    );

    let stdout = String::from_utf8_lossy(&gate.stdout);
    assert!(stdout.contains("<score>100</score>"), "{stdout}");
    assert!(stdout.contains("<status>ok</status>"), "{stdout}");
}

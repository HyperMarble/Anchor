use std::fs;
use std::process::Command;

#[test]
fn cli_status_summarizes_context_and_edit_events() {
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

    let context = Command::new(anchor)
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
        "context failed: {}",
        String::from_utf8_lossy(&context.stderr)
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
        "edit failed: {}\n{}",
        String::from_utf8_lossy(&edit.stderr),
        String::from_utf8_lossy(&edit.stdout)
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
    assert!(
        stdout.contains("<context_reads>1</context_reads>"),
        "{stdout}"
    );
    assert!(stdout.contains("<edits>1</edits>"), "{stdout}");
    assert!(stdout.contains("<path>src.rs</path>"), "{stdout}");
    assert!(stdout.contains("<symbol>login</symbol>"), "{stdout}");
    assert!(
        stdout.contains("<signal name=\"context_used\" status=\"ok\"/>"),
        "{stdout}"
    );
    assert!(
        stdout.contains("<signal name=\"edits_applied\" status=\"ok\"/>"),
        "{stdout}"
    );
}

use std::fs;
use std::process::Command;

#[test]
fn cli_context_returns_cached_marker_on_second_unchanged_symbol_read() {
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

    let first = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("login")
        .arg("--limit")
        .arg("1")
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "first context failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("<code>"), "{first_stdout}");
    assert!(first_stdout.contains("pub fn login"), "{first_stdout}");

    let second = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("login")
        .arg("--limit")
        .arg("1")
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "second context failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(
        second_stdout.contains("<symbol cached=\"true\">"),
        "{second_stdout}"
    );
    assert!(
        second_stdout.contains("<cache>CACHED</cache>"),
        "{second_stdout}"
    );
    assert!(!second_stdout.contains("<code>"), "{second_stdout}");
}

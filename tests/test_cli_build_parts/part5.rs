#[test]
fn cli_build_skips_binary_assets_before_utf8_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs/images")).unwrap();
    fs::write(dir.path().join("src.py"), "def login():\n    return True\n").unwrap();
    fs::write(dir.path().join("docs/images/logo.png"), [0, 159, 146, 150]).unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let build = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();

    assert!(
        build.status.success(),
        "build failed: {}\n{}",
        String::from_utf8_lossy(&build.stderr),
        String::from_utf8_lossy(&build.stdout)
    );
    let stderr = String::from_utf8_lossy(&build.stderr);
    assert!(!stderr.contains("read fail"), "{stderr}");

    let stdout = String::from_utf8_lossy(&build.stdout);
    assert!(stdout.contains("<files>1</files>"), "{stdout}");
}

#[test]
fn cli_context_truncates_large_default_symbol_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("def noisy():\n");
    for i in 0..180 {
        source.push_str(&format!("    print({i})\n"));
    }
    fs::write(dir.path().join("src.py"), source).unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let build = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}\n{}",
        String::from_utf8_lossy(&build.stderr),
        String::from_utf8_lossy(&build.stdout)
    );

    let context = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("context")
        .arg("noisy")
        .arg("--limit")
        .arg("1")
        .output()
        .unwrap();
    assert!(
        context.status.success(),
        "context failed: {}\n{}",
        String::from_utf8_lossy(&context.stderr),
        String::from_utf8_lossy(&context.stdout)
    );

    let stdout = String::from_utf8_lossy(&context.stdout);
    assert!(stdout.contains("context truncated"), "{stdout}");
    assert!(!stdout.contains("print(179)"), "{stdout}");
}

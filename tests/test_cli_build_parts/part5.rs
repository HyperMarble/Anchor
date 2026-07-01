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

#[test]
fn cli_build_writes_project_profile_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("lockd")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"profile-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/app.py"),
        "def repair_prompt():\n    return True\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("main.go"),
        "package main\n\nfunc main() {}\n",
    )
    .unwrap();
    fs::write(dir.path().join("lockd/go.mod"), "module example.com/lockd\n").unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"express":"5.0.0","react":"18.0.0"},"devDependencies":{"jest":"29.0.0"}}"#,
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
        "build failed: {}\n{}",
        String::from_utf8_lossy(&build.stderr),
        String::from_utf8_lossy(&build.stdout)
    );

    let profile_path = dir.path().join(".anchor/project_profile.json");
    let profile: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&profile_path).unwrap()).unwrap();

    assert_eq!(profile["schema"], "anchor.project_profile.v1", "{profile}");
    assert_eq!(profile["indexed_symbols"], 2, "{profile}");
    assert!(
        profile["indexed_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "src/app.py"),
        "{profile}"
    );
    assert!(
        profile["indexed_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "package.json"),
        "{profile}"
    );
    assert!(
        profile["manifests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "Cargo.toml"),
        "{profile}"
    );
    assert!(
        profile["test_commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "cargo test"),
        "{profile}"
    );
    assert!(
        profile["frameworks_present"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "react"),
        "{profile}"
    );
    assert!(
        profile["frameworks_present"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "jest"),
        "{profile}"
    );
    assert!(
        profile["frameworks_absent"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "next"),
        "{profile}"
    );
    assert!(
        profile["entrypoints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "main.go"),
        "{profile}"
    );
}

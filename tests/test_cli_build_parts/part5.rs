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
fn cli_build_writes_instruction_file_product_memory() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".cursor/rules")).unwrap();
    fs::create_dir_all(dir.path().join(".continue/rules")).unwrap();
    fs::create_dir_all(dir.path().join(".github")).unwrap();
    fs::write(dir.path().join("src.py"), "def repair():\n    return True\n").unwrap();
    fs::write(
        dir.path().join("AGENTS.md"),
        "# Repo rules\nAlways inspect tests before editing.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("CLAUDE.md"),
        "# Claude rules\nStay grounded in repo files.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".cursor/rules/prompt-repair.mdc"),
        "# Cursor rule\nPrefer prompt repair evidence.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".continue/rules/repair.md"),
        "# Continue rule\nKeep edits scoped.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".github/copilot-instructions.md"),
        "# Copilot\nUse repo-local checks.\n",
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

    let memory_path = dir.path().join(".anchor/product_memory.json");
    let memory = fs::read_to_string(&memory_path)
        .unwrap_or_else(|e| panic!("missing product memory {}: {e}", memory_path.display()));
    let json: serde_json::Value = serde_json::from_str(&memory).unwrap();
    assert_eq!(json["schema"], "anchor.product_memory.v1", "{memory}");

    let files = json["instruction_files"].as_array().unwrap();
    let paths = files
        .iter()
        .map(|item| item["path"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            ".continue/rules/repair.md",
            ".cursor/rules/prompt-repair.mdc",
            ".github/copilot-instructions.md",
            "AGENTS.md",
            "CLAUDE.md",
        ],
        "{memory}"
    );

    for item in files {
        let kind = item["kind"].as_str().unwrap();
        let note = item["note"].as_str().unwrap();
        let source_hash = item["source_hash"].as_str().unwrap();
        assert!(!kind.is_empty(), "{memory}");
        assert!(note.contains("instruction") || note.contains("prompt repair"), "{memory}");
        assert_eq!(source_hash.len(), 64, "{memory}");
    }
}

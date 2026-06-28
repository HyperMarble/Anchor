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
fn cli_build_writes_product_memory_cache_from_local_docs() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"anchor-fixture\"\nversion = \"0.1.0\"\ndescription = \"Repo-local execution harness for coding AI agents.\"\nkeywords = [\"ai\", \"agents\", \"coding\"]\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("README.md"),
        "# Anchor Fixture\n\nAnchor is a repo-local execution harness for coding agents working inside real codebases.\n\nAnchor writes its local index to `.anchor/`.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("docs/prompt-repair.md"),
        "# Prompt Repair\n\nPrompt Repair is experimental.\n\nThe default path should not call an LLM.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src.py"),
        "def login():\n    return True\n",
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

    let stdout = String::from_utf8_lossy(&build.stdout);
    assert!(stdout.contains("<product_memory_facts>"), "{stdout}");

    let memory_path = dir.path().join(".anchor/product_memory.json");
    let memory: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&memory_path).unwrap()).unwrap();
    assert_eq!(memory["schema"], "anchor.product_memory.v1", "{memory}");
    assert!(
        memory["source_hash"]
            .as_str()
            .map(|value| !value.is_empty())
            .unwrap_or(false),
        "{memory}"
    );
    let facts = memory["facts"].as_array().unwrap();
    assert!(
        facts.iter().any(|fact| {
            fact["source"] == "README.md"
                && fact["fact"]
                    .as_str()
                    .unwrap_or("")
                    .contains("repo-local execution harness")
        }),
        "{memory}"
    );
    assert!(
        facts.iter().any(|fact| {
            fact["source"] == "docs/prompt-repair.md"
                && fact["fact"]
                    .as_str()
                    .unwrap_or("")
                    .contains("should not call an LLM")
        }),
        "{memory}"
    );
    assert!(
        facts.iter().any(|fact| {
            fact["source"] == "Cargo.toml"
                && fact["fact"]
                    .as_str()
                    .unwrap_or("")
                    .contains("coding AI agents")
        }),
        "{memory}"
    );
}

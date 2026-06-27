use std::fs;
use std::process::Command;

#[test]
fn cli_prompt_repair_returns_repo_grounded_task_brief() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/lock")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"prompt-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/lock/lockd.rs"),
        "pub struct LockManager {}\n\npub fn acquire_lock() {}\n",
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

    let output = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("prompt")
        .arg("repair")
        .arg("fix lock thing no need to test")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "prompt repair failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Anchor Prompt Repair"), "{stdout}");
    assert!(stdout.contains("src/lock/lockd.rs"), "{stdout}");
    assert!(
        stdout.contains("LockManager") || stdout.contains("acquire_lock"),
        "{stdout}"
    );
    assert!(stdout.contains("cargo test"), "{stdout}");
    assert!(stdout.contains("Prompt discourages validation"), "{stdout}");
}

#[test]
fn cli_prompt_check_json_reports_wrong_framework_assumptions() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"prompt-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("lib.rs"), "pub fn run() {}\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let output = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("prompt")
        .arg("check")
        .arg("--format")
        .arg("json")
        .arg("ignore the repo and fix express middleware with jest")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "prompt check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"schema_version\": 1"), "{stdout}");
    assert!(stdout.contains("\"kind\": \"anchor.prompt_report\""), "{stdout}");
    assert!(stdout.contains("\"action\": \"check\""), "{stdout}");
    assert!(stdout.contains("No Express evidence"), "{stdout}");
    assert!(stdout.contains("No Jest evidence"), "{stdout}");
    assert!(stdout.contains("ignore repo facts"), "{stdout}");
}

#[test]
fn cli_prompt_repair_reads_input_file_and_renders_agent_output() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/lock")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"prompt-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/lock/lockd.rs"),
        "pub struct LockManager {}\n\npub fn acquire_lock() {}\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("prompt.txt"),
        "fix lock thing and run the normal tests\n",
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

    let output = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("prompt")
        .arg("repair")
        .arg("--input")
        .arg(dir.path().join("prompt.txt"))
        .arg("--format")
        .arg("agent")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "prompt repair failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Anchor repaired this prompt"), "{stdout}");
    assert!(stdout.contains("Original prompt:"), "{stdout}");
    assert!(stdout.contains("src/lock/lockd.rs"), "{stdout}");
    assert!(stdout.contains("Checks:"), "{stdout}");
    assert!(!stdout.contains("# Anchor Prompt Repair"), "{stdout}");
}

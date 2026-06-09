use std::fs;
use std::process::Command;

fn init_git_repo(path: &std::path::Path) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("init")
        .arg("-q")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.email")
        .arg("anchor-test@example.invalid")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("config")
        .arg("user.name")
        .arg("Anchor Test")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("add")
        .arg("-A")
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("commit")
        .arg("-q")
        .arg("-m")
        .arg("base")
        .status()
        .unwrap()
        .success());
}

#[test]
fn cli_run_blocks_raw_terminal_file_mutation() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("src.rs"),
        "pub fn value() -> bool {\n    true\n}\n",
    )
    .unwrap();
    init_git_repo(dir.path());

    let anchor = env!("CARGO_BIN_EXE_anchor");
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .status()
        .unwrap()
        .success());

    let run = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("run")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("printf 'pub fn value() -> bool {\\n    false\\n}\\n' > src.rs")
        .output()
        .unwrap();
    assert!(
        !run.status.success(),
        "raw mutation should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("<raw_changed_file>src.rs</raw_changed_file>"));

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(receipt.status.success());
    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(flags.iter().any(|flag| flag == "raw_terminal_write"));
    assert!(flags.iter().any(|flag| flag == "unrecorded_repo_changes"));
    assert_eq!(json["summary"]["raw_terminal_writes"], 1);
    assert_eq!(json["summary"]["raw_terminal_write_paths"][0], "src.rs");
}

#[test]
fn cli_run_allows_terminal_command_that_mutates_through_anchor() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("src.rs"),
        "pub fn value() -> bool {\n    true\n}\n",
    )
    .unwrap();
    init_git_repo(dir.path());

    let anchor = env!("CARGO_BIN_EXE_anchor");
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("build")
        .status()
        .unwrap()
        .success());

    let run = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("run")
        .arg("--")
        .arg(anchor)
        .arg("--root")
        .arg(".")
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("value")
        .arg("--content")
        .arg("pub fn value() -> bool {\n    false\n}")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "Anchor-routed mutation should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("<raw_changed_files>0</raw_changed_files>"));

    let receipt = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("receipt")
        .output()
        .unwrap();
    assert!(receipt.status.success());
    let json: serde_json::Value = serde_json::from_slice(&receipt.stdout).unwrap();
    let flags = json["quality"]["flags"].as_array().unwrap();
    assert!(!flags.iter().any(|flag| flag == "raw_terminal_write"));
    assert_eq!(json["summary"]["raw_terminal_writes"], 0);
}

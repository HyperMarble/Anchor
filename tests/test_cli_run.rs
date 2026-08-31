use std::fs;
use std::process::Command;

#[path = "test_cli_support.rs"]
mod support;
use support::init_git_repo;

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

    let mut run_cmd = Command::new(anchor);
    run_cmd.arg("--root").arg(dir.path()).arg("run").arg("--");
    support::write_line("src.rs", "mutated").apply(&mut run_cmd);
    let run = run_cmd.output().unwrap();
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
fn cli_run_allows_read_only_terminal_command() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("src.rs"),
        "pub fn value() -> bool {\n    true\n}\n",
    )
    .unwrap();
    init_git_repo(dir.path());

    let anchor = env!("CARGO_BIN_EXE_anchor");

    let mut run_cmd = Command::new(anchor);
    run_cmd.arg("--root").arg(dir.path()).arg("run").arg("--");
    support::read_file_to_null("src.rs").apply(&mut run_cmd);
    let run = run_cmd.output().unwrap();
    assert!(
        run.status.success(),
        "read-only command should pass\nstdout:\n{}\nstderr:\n{}",
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

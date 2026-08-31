use std::fs;
use std::process::Command;

#[path = "test_cli_support.rs"]
mod support;
use support::init_git_repo;

fn patch_path(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("<patch_path>")
                .and_then(|rest| rest.strip_suffix("</patch_path>"))
        })
        .expect("missing patch_path")
        .to_string()
}

#[test]
fn cli_run_execroot_captures_patch_without_touching_real_repo() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("src.txt"), "old\n").unwrap();
    init_git_repo(dir.path());

    let anchor = env!("CARGO_BIN_EXE_anchor");

    let mut run_cmd = Command::new(anchor);
    run_cmd
        .env("ANCHOR_EXECROOT", "1")
        .arg("--root")
        .arg(dir.path())
        .arg("run")
        .arg("--");
    support::write_line("src.txt", "new").apply(&mut run_cmd);
    let run = run_cmd.output().unwrap();
    assert!(
        run.status.success(),
        "execroot run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("<mode>execroot</mode>"), "{stdout}");
    assert!(
        stdout.contains("<changed_file>src.txt</changed_file>"),
        "{stdout}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("src.txt")).unwrap(),
        "old\n"
    );

    let patch = fs::read_to_string(patch_path(&stdout)).unwrap();
    assert!(patch.contains("-old"), "{patch}");
    assert!(patch.contains("+new"), "{patch}");
}

#[test]
fn cli_run_execroot_captures_new_files_as_patch() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("src.txt"), "base\n").unwrap();
    init_git_repo(dir.path());

    let anchor = env!("CARGO_BIN_EXE_anchor");

    let mut run_cmd = Command::new(anchor);
    run_cmd
        .env("ANCHOR_EXECROOT", "1")
        .arg("--root")
        .arg(dir.path())
        .arg("run")
        .arg("--");
    support::write_line("created.txt", "created").apply(&mut run_cmd);
    let run = run_cmd.output().unwrap();
    assert!(
        run.status.success(),
        "execroot run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("<changed_file>created.txt</changed_file>"),
        "{stdout}"
    );
    assert!(!dir.path().join("created.txt").exists());

    let patch = fs::read_to_string(patch_path(&stdout)).unwrap();
    assert!(patch.contains("created.txt"), "{patch}");
    assert!(patch.contains("+created"), "{patch}");
}

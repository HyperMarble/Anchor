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

    let run = Command::new(anchor)
        .env("ANCHOR_EXECROOT", "1")
        .arg("--root")
        .arg(dir.path())
        .arg("run")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("printf 'new\\n' > src.txt")
        .output()
        .unwrap();
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

    let run = Command::new(anchor)
        .env("ANCHOR_EXECROOT", "1")
        .arg("--root")
        .arg(dir.path())
        .arg("run")
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg("printf 'created\\n' > created.txt")
        .output()
        .unwrap();
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

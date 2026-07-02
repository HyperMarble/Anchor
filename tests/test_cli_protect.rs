use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct ProtectCleanup {
    anchor: &'static str,
    root: PathBuf,
}

impl Drop for ProtectCleanup {
    fn drop(&mut self) {
        let _ = Command::new(self.anchor)
            .arg("--root")
            .arg(&self.root)
            .arg("protect")
            .arg("off")
            .output();
    }
}

#[test]
fn cli_protect_blocks_raw_source_writes() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let source = src_dir.join("app.py");
    fs::write(&source, "def value():\n    return 1\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    let protect = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("protect")
        .arg("on")
        .output()
        .unwrap();
    assert!(
        protect.status.success(),
        "protect failed: {}\n{}",
        String::from_utf8_lossy(&protect.stderr),
        String::from_utf8_lossy(&protect.stdout)
    );
    let _cleanup = ProtectCleanup {
        anchor,
        root: dir.path().to_path_buf(),
    };

    assert!(
        fs::write(&source, "def value():\n    return 2\n").is_err(),
        "raw source write should fail while protection is active"
    );

    let status = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("protect")
        .arg("status")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("<status>active</status>"), "{stdout}");
}

#[test]
fn cli_protect_off_restores_source_writes() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let source = src_dir.join("app.py");
    fs::write(&source, "def value():\n    return 1\n").unwrap();

    let anchor = env!("CARGO_BIN_EXE_anchor");
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("protect")
        .arg("on")
        .status()
        .unwrap()
        .success());
    assert!(Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("protect")
        .arg("off")
        .status()
        .unwrap()
        .success());

    fs::write(&source, "def value():\n    return 2\n").unwrap();
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "def value():\n    return 2\n"
    );
}

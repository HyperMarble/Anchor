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
fn cli_protect_blocks_raw_source_writes_but_allows_anchor_edit() {
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

    let tmp = src_dir.join("app.py.tmp");
    assert!(
        fs::write(&tmp, "def value():\n    return 3\n").is_err()
            || fs::rename(&tmp, &source).is_err(),
        "raw temp-file replacement should fail while source directory is protected"
    );
    let _ = fs::remove_file(&tmp);

    let edit = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src/app.py")
        .arg("--action")
        .arg("replace")
        .arg("--pattern")
        .arg("return 1")
        .arg("--content")
        .arg("return 4")
        .output()
        .unwrap();
    assert!(
        edit.status.success(),
        "anchor edit failed: {}\n{}",
        String::from_utf8_lossy(&edit.stderr),
        String::from_utf8_lossy(&edit.stdout)
    );
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "def value():\n    return 4\n"
    );
    assert!(
        fs::write(&source, "def value():\n    return 5\n").is_err(),
        "anchor edit should relock source file"
    );
}

#[test]
fn cli_protect_allows_anchor_write_to_create_new_source_and_relocks_it() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("existing.py"), "VALUE = 1\n").unwrap();

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

    let write = Command::new(anchor)
        .arg("--root")
        .arg(dir.path())
        .arg("write")
        .arg("src/new_module.py")
        .arg("VALUE = 2\n")
        .output()
        .unwrap();
    assert!(
        write.status.success(),
        "anchor write failed: {}\n{}",
        String::from_utf8_lossy(&write.stderr),
        String::from_utf8_lossy(&write.stdout)
    );

    let new_source = src_dir.join("new_module.py");
    assert_eq!(fs::read_to_string(&new_source).unwrap(), "VALUE = 2\n");
    assert!(
        fs::write(&new_source, "VALUE = 3\n").is_err(),
        "new source file should be relocked after anchor write"
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

#[test]
fn cli_symbol_edit_blocks_when_another_agent_holds_same_symbol_lock() {
    let dir = tempfile::tempdir().unwrap();
    build_repo(&dir);
    let lockd = MockLockd::start();
    let anchor = env!("CARGO_BIN_EXE_anchor");
    let held_symbol = symbol_lock("src.rs", "target");

    lockd.hold(&held_symbol, "src.rs", "agent-a");

    let blocked = Command::new(anchor)
        .env("ANCHOR_LOCKD_SOCKET", lockd.path())
        .env("ANCHOR_AGENT_ID", "agent-b")
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("target")
        .arg("--content")
        .arg("pub fn target() -> bool {\n    false\n}")
        .output()
        .unwrap();

    assert!(
        !blocked.status.success(),
        "agent-b should be blocked while agent-a owns target lock\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&blocked.stdout),
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(
        String::from_utf8_lossy(&blocked.stderr).contains("BLOCKED by agent-a"),
        "{}",
        String::from_utf8_lossy(&blocked.stderr)
    );

    let events = read_events(&dir);
    assert!(
        events.iter().any(|event| {
            event["event_type"] == "lock.acquire"
                && event["status"] == "blocked"
                && event["symbol"] == "target"
                && event["path"] == "src.rs"
        }),
        "missing blocked lock event: {events:#?}"
    );

    let unchanged = fs::read_to_string(dir.path().join("src.rs")).unwrap();
    assert!(unchanged.contains("pub fn target() -> bool {\n    true\n}"));

    lockd.clear();
    let allowed = Command::new(anchor)
        .env("ANCHOR_LOCKD_SOCKET", lockd.path())
        .env("ANCHOR_AGENT_ID", "agent-b")
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("target")
        .arg("--content")
        .arg("pub fn target() -> bool {\n    false\n}")
        .output()
        .unwrap();

    assert!(
        allowed.status.success(),
        "agent-b should edit after lock is released\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&allowed.stdout),
        String::from_utf8_lossy(&allowed.stderr)
    );
    let changed = fs::read_to_string(dir.path().join("src.rs")).unwrap();
    assert!(changed.contains("pub fn target() -> bool {\n    false\n}"));

    let events = read_events(&dir);
    assert!(
        events.iter().any(|event| {
            event["event_type"] == "lock.acquire"
                && event["status"] == "ok"
                && event["symbol"] == "target"
                && event["path"] == "src.rs"
        }),
        "missing successful lock event: {events:#?}"
    );
    assert!(
        events.iter().any(|event| {
            event["event_type"] == "lock.release"
                && event["status"] == "ok"
                && event["symbol"] == "target"
                && event["path"] == "src.rs"
        }),
        "missing lock release event: {events:#?}"
    );
}

#[test]
fn cli_symbol_edit_allows_different_symbol_while_other_symbol_is_locked() {
    let dir = tempfile::tempdir().unwrap();
    build_repo(&dir);
    let lockd = MockLockd::start();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    lockd.hold(symbol_lock("src.rs", "target"), "src.rs", "agent-a");

    let edit = Command::new(anchor)
        .env("ANCHOR_LOCKD_SOCKET", lockd.path())
        .env("ANCHOR_AGENT_ID", "agent-b")
        .arg("--root")
        .arg(dir.path())
        .arg("edit")
        .arg("src.rs")
        .arg("--symbol")
        .arg("other")
        .arg("--content")
        .arg("pub fn other() -> bool {\n    true\n}")
        .output()
        .unwrap();

    assert!(
        edit.status.success(),
        "agent-b should edit independent symbol\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&edit.stdout),
        String::from_utf8_lossy(&edit.stderr)
    );

    let source = fs::read_to_string(dir.path().join("src.rs")).unwrap();
    assert!(source.contains("pub fn target() -> bool {\n    true\n}"));
    assert!(source.contains("pub fn other() -> bool {\n    true\n}"));
}

#[test]
fn cli_write_blocks_when_another_agent_holds_file_lock() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("src.rs"), "pub fn keep() {}\n").unwrap();
    let lockd = MockLockd::start();
    let anchor = env!("CARGO_BIN_EXE_anchor");

    lockd.hold("__file__", "src.rs", "agent-a");

    let blocked = Command::new(anchor)
        .env("ANCHOR_LOCKD_SOCKET", lockd.path())
        .env("ANCHOR_AGENT_ID", "agent-b")
        .arg("--root")
        .arg(dir.path())
        .arg("write")
        .arg("src.rs")
        .arg("pub fn replaced() {}\n")
        .output()
        .unwrap();

    assert!(
        !blocked.status.success(),
        "agent-b should be blocked by file lock\nstdout:\n{}\nstderr:\n{}\nseen:\n{:?}",
        String::from_utf8_lossy(&blocked.stdout),
        String::from_utf8_lossy(&blocked.stderr),
        lockd.seen()
    );
    let source = fs::read_to_string(dir.path().join("src.rs")).unwrap();
    assert_eq!(source, "pub fn keep() {}\n");
}

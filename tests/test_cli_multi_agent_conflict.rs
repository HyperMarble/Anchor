use anchor::storage::content_hash;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(0);

struct MockLockd {
    socket: PathBuf,
    held: Arc<Mutex<HashMap<(String, String), String>>>,
    seen: Arc<Mutex<Vec<(String, String, String, String)>>>,
}

impl MockLockd {
    fn start() -> Self {
        let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);
        let socket = PathBuf::from("/private/tmp").join(format!(
            "anchor-lockd-cli-conflict-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                + u128::from(id)
        ));
        let _ = fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();

        let held: Arc<Mutex<HashMap<(String, String), String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let seen: Arc<Mutex<Vec<(String, String, String, String)>>> =
            Arc::new(Mutex::new(Vec::new()));
        let held_for_thread = held.clone();
        let seen_for_thread = seen.clone();
        let socket_for_thread = socket.clone();

        thread::spawn(move || {
            for _ in 0..500 {
                match listener.accept() {
                    Ok((stream, _)) => handle_conn(stream, &held_for_thread, &seen_for_thread),
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        if !socket_for_thread.exists() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self { socket, held, seen }
    }

    fn hold(&self, symbol: impl Into<String>, path: impl Into<String>, owner: impl Into<String>) {
        self.held
            .lock()
            .unwrap()
            .insert((symbol.into(), path.into()), owner.into());
    }

    fn clear(&self) {
        self.held.lock().unwrap().clear();
    }

    fn path(&self) -> &Path {
        &self.socket
    }

    fn seen(&self) -> Vec<(String, String, String, String)> {
        self.seen.lock().unwrap().clone()
    }
}

impl Drop for MockLockd {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket);
    }
}

fn handle_conn(
    mut stream: UnixStream,
    held: &Arc<Mutex<HashMap<(String, String), String>>>,
    seen: &Arc<Mutex<Vec<(String, String, String, String)>>>,
) {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            return;
        }
    }

    let req: Value = serde_json::from_str(line.trim()).unwrap();
    let op = req["op"].as_str().unwrap_or("");
    let symbol = req["symbol"].as_str().unwrap_or("");
    let path = req["path"].as_str().unwrap_or("");
    let agent = req["agent"].as_str().unwrap_or("");
    seen.lock().unwrap().push((
        op.to_string(),
        symbol.to_string(),
        path.to_string(),
        agent.to_string(),
    ));

    let resp = match op {
        "acquire" => {
            let mut guard = held.lock().unwrap();
            let key = (symbol.to_string(), path.to_string());
            match guard.get(&key) {
                Some(owner) if owner != agent => {
                    serde_json::json!({
                        "ok": false,
                        "code": "locked",
                        "owner": owner,
                        "detail": "locked by another agent"
                    })
                }
                _ => {
                    guard.insert(key, agent.to_string());
                    serde_json::json!({ "ok": true })
                }
            }
        }
        "release" => {
            let mut guard = held.lock().unwrap();
            let key = (symbol.to_string(), path.to_string());
            if guard.get(&key).map(|owner| owner == agent).unwrap_or(false) {
                guard.remove(&key);
            }
            serde_json::json!({ "ok": true })
        }
        _ => serde_json::json!({ "ok": false, "code": "unknown_op" }),
    };

    let _ = writeln!(stream, "{resp}");
}

fn build_repo(dir: &tempfile::TempDir) {
    let source = dir.path().join("src.rs");
    fs::write(
        &source,
        "pub fn target() -> bool {\n    true\n}\n\npub fn other() -> bool {\n    false\n}\n",
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
}

fn symbol_lock(path: &str, symbol: &str) -> String {
    format!(
        "sym:{}",
        content_hash(format!("{path}\0{symbol}").as_bytes())
    )
}

fn read_events(dir: &tempfile::TempDir) -> Vec<Value> {
    let events_path = dir.path().join(".anchor/events/events.jsonl");
    let raw_events = fs::read_to_string(&events_path)
        .unwrap_or_else(|e| panic!("missing event log {}: {e}", events_path.display()));
    raw_events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

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

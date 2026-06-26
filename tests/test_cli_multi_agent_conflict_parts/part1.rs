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

type LockKey = (String, String);
type SeenRequest = (String, String, String, String);
type HeldLocks = Arc<Mutex<HashMap<LockKey, String>>>;
type SeenRequests = Arc<Mutex<Vec<SeenRequest>>>;

struct MockLockd {
    socket: PathBuf,
    held: HeldLocks,
    seen: SeenRequests,
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

        let held: HeldLocks = Arc::new(Mutex::new(HashMap::new()));
        let seen: SeenRequests = Arc::new(Mutex::new(Vec::new()));
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

    fn seen(&self) -> Vec<SeenRequest> {
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
    held: &HeldLocks,
    seen: &SeenRequests,
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

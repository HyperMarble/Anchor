//
//  lockd.rs
//  Anchor
//
//  Client for the anchor-lockd local IPC daemon.
//  Tries lockd first; callers fall back to in-process LockManager if unavailable.
//

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

#[cfg(unix)]
const SOCKET_PATH: &str = "/tmp/anchor.lock.sock";
const TIMEOUT: Duration = Duration::from_millis(500);
const AGENT_ID_ENV: &str = "ANCHOR_AGENT_ID";
const SOCKET_PATH_ENV: &str = "ANCHOR_LOCKD_SOCKET";
const MAX_RESPONSE_SIZE: u64 = 1 << 20;

static AGENT_ID: OnceLock<String> = OnceLock::new();
static WORKSPACE_AGENT_ID: OnceLock<String> = OnceLock::new();

#[derive(Debug)]
pub enum LockdResult {
    Acquired,
    Blocked { owner: String, reason: String },
    Unavailable,
}

fn send(req: serde_json::Value) -> Option<serde_json::Value> {
    transport::send(endpoint(), req, TIMEOUT)
}

/// Owner used for lockd requests from this CLI session.
///
/// Set ANCHOR_AGENT_ID to make multiple CLI calls part of the same agent session.
/// Otherwise Anchor derives a stable local ID from the current workspace. Anchor
/// is a CLI, so a process-local default would make every command look like a
/// different agent and break provenance across `context`, `edit`, and `check`.
pub fn agent_id() -> &'static str {
    AGENT_ID
        .get_or_init(|| {
            std::env::var(AGENT_ID_ENV)
                .ok()
                .and_then(|value| normalize_agent_id(&value))
                .or_else(|| WORKSPACE_AGENT_ID.get().cloned())
                .unwrap_or_else(default_agent_id)
        })
        .as_str()
}

pub fn set_workspace(root: &Path) {
    let _ = WORKSPACE_AGENT_ID.set(workspace_agent_id(root));
}

fn default_agent_id() -> String {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|path| path.canonicalize().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    workspace_agent_id(&cwd)
}

fn workspace_agent_id(root: &Path) -> String {
    let root = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .to_string();
    let user = current_user();
    normalize_agent_id(&format!(
        "anchor-{user}-{:016x}",
        stable_hash(root.as_bytes())
    ))
    .unwrap_or_else(|| "anchor".to_string())
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn normalize_agent_id(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len().min(64));
    let mut last_was_dash = false;

    for ch in raw.chars() {
        let ch = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };
        if ch == '-' {
            if out.is_empty() || last_was_dash {
                continue;
            }
            last_was_dash = true;
        } else {
            last_was_dash = false;
        }
        out.push(ch);
        if out.len() >= 64 {
            break;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    (!out.is_empty()).then_some(out)
}

fn acquire_request(symbol: &str, path: &str, agent: &str) -> serde_json::Value {
    serde_json::json!({
        "op": "acquire",
        "symbol": symbol,
        "path": path,
        "agent": agent
    })
}

fn release_request(symbol: &str, path: &str, agent: &str) -> serde_json::Value {
    serde_json::json!({
        "op": "release",
        "symbol": symbol,
        "path": path,
        "agent": agent
    })
}

/// Try to acquire a symbol lock via lockd.
pub fn acquire(symbol: &str, path: &str) -> LockdResult {
    acquire_for_agent(symbol, path, agent_id())
}

/// Try to acquire a symbol lock via lockd for a specific agent/session owner.
pub fn acquire_for_agent(symbol: &str, path: &str, agent: &str) -> LockdResult {
    let resp = send(acquire_request(symbol, path, agent));

    match resp {
        None => LockdResult::Unavailable,
        Some(r) => {
            if r["ok"] == true {
                LockdResult::Acquired
            } else {
                let owner = r["owner"].as_str().unwrap_or("unknown").to_string();
                let reason = r["detail"]
                    .as_str()
                    .or_else(|| r["code"].as_str())
                    .unwrap_or("locked by another agent")
                    .to_string();
                LockdResult::Blocked { owner, reason }
            }
        }
    }
}

/// Release a symbol lock via lockd (best-effort).
pub fn release(symbol: &str, path: &str) {
    release_for_agent(symbol, path, agent_id());
}

/// Release a symbol lock via lockd for a specific agent/session owner.
pub fn release_for_agent(symbol: &str, path: &str, agent: &str) {
    let _ = send(release_request(symbol, path, agent));
}

/// Check if lockd is reachable.
pub fn is_available() -> bool {
    transport::is_available(endpoint(), TIMEOUT)
}

fn endpoint() -> String {
    std::env::var(SOCKET_PATH_ENV).unwrap_or_else(|_| default_endpoint())
}

#[cfg(unix)]
fn default_endpoint() -> String {
    SOCKET_PATH.to_string()
}

#[cfg(windows)]
fn default_endpoint() -> String {
    let user = normalize_agent_id(&current_user()).unwrap_or_else(|| "local".to_string());
    format!(r"\\.\pipe\anchor-lockd-{user}")
}

fn current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "local".to_string())
}

#[cfg(unix)]
mod transport {
    use super::MAX_RESPONSE_SIZE;
    use serde_json::Value;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    pub(super) fn send(endpoint: String, req: Value, timeout: Duration) -> Option<Value> {
        let mut stream = UnixStream::connect(endpoint).ok()?;
        stream.set_read_timeout(Some(timeout)).ok()?;
        stream.set_write_timeout(Some(timeout)).ok()?;
        exchange(&mut stream, req)
    }

    fn exchange(stream: &mut UnixStream, req: Value) -> Option<Value> {
        let mut line = req.to_string();
        line.push('\n');
        stream.write_all(line.as_bytes()).ok()?;

        let mut reader = BufReader::new(stream).take(MAX_RESPONSE_SIZE);
        let mut resp = String::new();
        reader.read_line(&mut resp).ok()?;
        serde_json::from_str(resp.trim()).ok()
    }

    pub(super) fn is_available(endpoint: String, _timeout: Duration) -> bool {
        UnixStream::connect(endpoint).is_ok()
    }
}

#[cfg(windows)]
mod transport {
    use super::MAX_RESPONSE_SIZE;
    use serde_json::Value;
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    // std exposes Windows named pipes as files. Run the complete exchange on a
    // worker so an unresponsive or partially-started daemon cannot hold up the
    // CLI beyond Anchor's IPC timeout.
    pub(super) fn send(endpoint: String, req: Value, timeout: Duration) -> Option<Value> {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(exchange(&endpoint, req));
        });
        rx.recv_timeout(timeout).ok().flatten()
    }

    fn exchange(endpoint: &str, req: Value) -> Option<Value> {
        let mut pipe = OpenOptions::new()
            .read(true)
            .write(true)
            .open(endpoint)
            .ok()?;
        let mut line = req.to_string();
        line.push('\n');
        pipe.write_all(line.as_bytes()).ok()?;
        pipe.flush().ok()?;

        let mut reader = BufReader::new(pipe).take(MAX_RESPONSE_SIZE);
        let mut resp = String::new();
        reader.read_line(&mut resp).ok()?;
        serde_json::from_str(resp.trim()).ok()
    }

    pub(super) fn is_available(endpoint: String, timeout: Duration) -> bool {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let available = OpenOptions::new()
                .read(true)
                .write(true)
                .open(endpoint)
                .is_ok();
            let _ = tx.send(available);
        });
        rx.recv_timeout(timeout).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_lockd_requests_use_supplied_agent_owner() {
        let acquire = acquire_request("Login", "src/auth.rs", "agent-123");
        assert_eq!(acquire["agent"], "agent-123");
        assert_eq!(acquire["op"], "acquire");

        let release = release_request("Login", "src/auth.rs", "agent-123");
        assert_eq!(release["agent"], "agent-123");
        assert_eq!(release["op"], "release");
    }

    #[test]
    fn regression_normalize_agent_id_matches_lockd_validator_shape() {
        assert_eq!(
            normalize_agent_id("codex:session_42 / worktree"),
            Some("codex-session-42-worktree".to_string())
        );
        assert_eq!(
            normalize_agent_id("Claude Code SESSION"),
            Some("claude-code-session".to_string())
        );
        assert_eq!(normalize_agent_id("---"), None);

        let long = normalize_agent_id(&"a".repeat(80)).unwrap();
        assert_eq!(long.len(), 64);
    }

    #[test]
    fn regression_default_agent_id_is_stable_for_cli_workflows() {
        let first = default_agent_id();
        let second = default_agent_id();

        assert_eq!(first, second);
        assert!(first.starts_with("anchor-"), "{first}");
        assert!(!first.contains(&std::process::id().to_string()));
    }

    #[cfg(windows)]
    #[test]
    fn regression_default_windows_endpoint_is_a_per_user_named_pipe() {
        let endpoint = default_endpoint();
        assert!(
            endpoint.starts_with(r"\\.\pipe\anchor-lockd-"),
            "{endpoint}"
        );
        assert!(!endpoint.ends_with('-'), "{endpoint}");
    }
}

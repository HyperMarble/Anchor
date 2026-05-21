//
//  lockd.rs
//  Anchor
//
//  Client for anchor-lockd Unix socket daemon.
//  Tries lockd first; callers fall back to in-process LockManager if unavailable.
//

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

const SOCKET_PATH: &str = "/tmp/anchor.lock.sock";
const TIMEOUT: Duration = Duration::from_millis(500);
const AGENT_ID_ENV: &str = "ANCHOR_AGENT_ID";

static AGENT_ID: OnceLock<String> = OnceLock::new();

#[derive(Debug)]
pub enum LockdResult {
    Acquired,
    Blocked { owner: String, reason: String },
    Unavailable,
}

fn send(req: serde_json::Value) -> Option<serde_json::Value> {
    let mut stream = UnixStream::connect(SOCKET_PATH).ok()?;
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;

    let mut line = req.to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).ok()?;

    let mut reader = BufReader::new(stream);
    let mut resp = String::new();
    reader.read_line(&mut resp).ok()?;
    serde_json::from_str(resp.trim()).ok()
}

/// Unique owner used for lockd requests from this process.
///
/// Set ANCHOR_AGENT_ID to make multiple CLI calls part of the same agent session.
/// Otherwise Anchor generates a process-local ID so unrelated agents do not all
/// look like the same owner to lockd.
pub fn agent_id() -> &'static str {
    AGENT_ID
        .get_or_init(|| {
            std::env::var(AGENT_ID_ENV)
                .ok()
                .and_then(|value| normalize_agent_id(&value))
                .unwrap_or_else(default_agent_id)
        })
        .as_str()
}

fn default_agent_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    normalize_agent_id(&format!("anchor-{}-{}", std::process::id(), millis))
        .unwrap_or_else(|| "anchor".to_string())
}

fn normalize_agent_id(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len().min(64));
    let mut last_was_dash = false;

    for ch in raw.chars() {
        let ch = if ch.is_ascii_alphanumeric() { ch } else { '-' };
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
    UnixStream::connect(SOCKET_PATH).is_ok()
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
        assert_eq!(normalize_agent_id("---"), None);

        let long = normalize_agent_id(&"a".repeat(80)).unwrap();
        assert_eq!(long.len(), 64);
    }
}

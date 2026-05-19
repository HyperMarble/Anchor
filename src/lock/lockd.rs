//
//  lockd.rs
//  Anchor
//
//  Client for anchor-lockd Unix socket daemon.
//  Tries lockd first; callers fall back to in-process LockManager if unavailable.
//

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const SOCKET_PATH: &str = "/tmp/anchor.lock.sock";
const TIMEOUT: Duration = Duration::from_millis(500);

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

/// Try to acquire a symbol lock via lockd.
pub fn acquire(symbol: &str, path: &str) -> LockdResult {
    let resp = send(serde_json::json!({
        "op": "acquire",
        "symbol": symbol,
        "path": path,
        "agent": "anchor-mcp"
    }));

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
    let _ = send(serde_json::json!({
        "op": "release",
        "symbol": symbol,
        "path": path,
        "agent": "anchor-mcp"
    }));
}

/// Check if lockd is reachable.
pub fn is_available() -> bool {
    UnixStream::connect(SOCKET_PATH).is_ok()
}

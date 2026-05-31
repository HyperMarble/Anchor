use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage::content_hash;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEvent {
    pub event_id: String,
    pub timestamp_ms: u128,
    pub session_id: String,
    pub agent_id: String,
    pub event_type: String,
    pub path: Option<String>,
    pub symbol: Option<String>,
    pub status: String,
    pub message: Option<String>,
}

impl ExecutionEvent {
    pub fn new(
        event_type: impl Into<String>,
        path: Option<String>,
        symbol: Option<String>,
        status: impl Into<String>,
        message: Option<String>,
    ) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let session_id = std::env::var("ANCHOR_SESSION_ID").unwrap_or_else(|_| "local".into());
        let agent_id = crate::lock::lockd::agent_id().to_string();
        let event_type = event_type.into();
        let status = status.into();
        let id_seed = format!(
            "{timestamp_ms}\0{session_id}\0{agent_id}\0{event_type}\0{:?}\0{:?}\0{status}",
            path, symbol
        );

        Self {
            event_id: content_hash(id_seed.as_bytes()),
            timestamp_ms,
            session_id,
            agent_id,
            event_type,
            path,
            symbol,
            status,
            message,
        }
    }
}

pub fn append(anchor_root: &Path, event: &ExecutionEvent) -> anyhow::Result<()> {
    let dir = anchor_root.join("events");
    fs::create_dir_all(&dir)?;
    let path = dir.join("events.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(event)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn log_path(anchor_root: &Path) -> PathBuf {
    anchor_root.join("events").join("events.jsonl")
}

pub fn load(anchor_root: &Path) -> anyhow::Result<Vec<ExecutionEvent>> {
    let path = log_path(anchor_root);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        events.push(serde_json::from_str(&line)?);
    }

    Ok(events)
}

pub fn record(
    anchor_root: &Path,
    event_type: impl Into<String>,
    path: Option<String>,
    symbol: Option<String>,
    status: impl Into<String>,
    message: Option<String>,
) {
    let event = ExecutionEvent::new(event_type, path, symbol, status, message);
    let _ = append(anchor_root, &event);
}

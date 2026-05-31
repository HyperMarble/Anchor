use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventSummary {
    pub event_count: usize,
    pub context_reads: usize,
    pub cache_hits: usize,
    pub edits_ok: usize,
    pub writes_ok: usize,
    pub checks_ok: usize,
    pub checks_failed: usize,
    pub errors: usize,
    pub lock_blocks: usize,
    pub paths: Vec<String>,
    pub symbols: Vec<String>,
    pub sessions: Vec<String>,
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityProfile {
    pub score: u8,
    pub risk: String,
    pub flags: Vec<String>,
}

impl EventSummary {
    pub fn from_events(events: &[ExecutionEvent]) -> Self {
        let mut summary = Self {
            event_count: events.len(),
            ..Self::default()
        };
        let mut paths = BTreeSet::new();
        let mut symbols = BTreeSet::new();
        let mut sessions = BTreeSet::new();
        let mut agents = BTreeSet::new();

        for event in events {
            sessions.insert(event.session_id.clone());
            agents.insert(event.agent_id.clone());
            if let Some(path) = &event.path {
                paths.insert(path.clone());
            }
            if let Some(symbol) = &event.symbol {
                symbols.insert(symbol.clone());
            }
            if event.status == "error" {
                summary.errors += 1;
            }

            match (event.event_type.as_str(), event.status.as_str()) {
                ("context.read", "ok") => summary.context_reads += 1,
                ("context.read", "cached") => {
                    summary.context_reads += 1;
                    summary.cache_hits += 1;
                }
                ("edit.apply", "ok") => summary.edits_ok += 1,
                ("write.apply", "ok") => summary.writes_ok += 1,
                ("check.run", "ok") => summary.checks_ok += 1,
                ("check.run", "error") => summary.checks_failed += 1,
                ("lock.acquire", "blocked") => summary.lock_blocks += 1,
                _ => {}
            }
        }

        summary.paths = paths.into_iter().collect();
        summary.symbols = symbols.into_iter().collect();
        summary.sessions = sessions.into_iter().collect();
        summary.agents = agents.into_iter().collect();
        summary
    }

    pub fn quality_profile(&self) -> QualityProfile {
        let mut score: i32 = 100;
        let mut flags = Vec::new();
        let changed = self.edits_ok + self.writes_ok > 0;
        let checked = self.checks_ok + self.checks_failed > 0;

        if changed && self.context_reads == 0 {
            score -= 30;
            flags.push("changed_without_recorded_context".to_string());
        }
        if changed && !checked {
            score -= 25;
            flags.push("changed_without_recorded_check".to_string());
        }
        if self.checks_failed > 0 {
            score -= 30;
            flags.push("failed_check".to_string());
        }
        if self.errors > 0 {
            score -= 20;
            flags.push("execution_error".to_string());
        }
        if self.lock_blocks > 0 {
            score -= 5;
            flags.push("lock_conflict_seen".to_string());
        }
        if self.paths.len() > 3 {
            score -= 10;
            flags.push("broad_file_scope".to_string());
        }

        let score = score.clamp(0, 100) as u8;
        let risk = if score >= 85 {
            "low"
        } else if score >= 60 {
            "medium"
        } else {
            "high"
        }
        .to_string();

        QualityProfile { score, risk, flags }
    }
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

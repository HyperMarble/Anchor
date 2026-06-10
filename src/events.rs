use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
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
    pub unresolved_checks_failed: usize,
    pub test_checks_ok: usize,
    pub test_checks_failed: usize,
    pub unresolved_test_checks_failed: usize,
    pub check_commands: Vec<String>,
    pub unresolved_check_commands: Vec<String>,
    pub check_target_paths: Vec<String>,
    pub errors: usize,
    pub unresolved_errors: usize,
    pub lock_blocks: usize,
    pub stale_write_blocks: usize,
    pub unresolved_stale_write_blocks: usize,
    pub unresolved_stale_write_paths: Vec<String>,
    pub guarded_writes: usize,
    pub edits_without_file_context: usize,
    pub unresolved_edits_without_file_context: usize,
    pub unresolved_edits_without_file_context_paths: Vec<String>,
    pub changed_line_total: usize,
    pub max_changed_lines: usize,
    pub oversized_edits: usize,
    pub raw_terminal_writes: usize,
    pub raw_terminal_write_paths: Vec<String>,
    pub changed_file_scope: usize,
    pub changed_file_scope_paths: Vec<String>,
    pub unrecorded_changed_files: usize,
    pub unrecorded_changed_file_list: Vec<String>,
    pub recorded_write_paths: Vec<String>,
    pub paths: Vec<String>,
    pub symbols: Vec<String>,
    pub sessions: Vec<String>,
    pub agents: Vec<String>,
    pub risky_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityProfile {
    pub score: u8,
    pub risk: String,
    pub flags: Vec<String>,
    pub recommendations: Vec<String>,
}

impl EventSummary {
    pub fn from_events(events: &[ExecutionEvent]) -> Self {
        let mut summary = Self {
            event_count: events.len(),
            ..Self::default()
        };
        let mut paths = BTreeSet::new();
        let mut recorded_write_paths = BTreeSet::new();
        let mut symbols = BTreeSet::new();
        let mut sessions = BTreeSet::new();
        let mut agents = BTreeSet::new();
        let mut risky_paths = BTreeSet::new();
        let mut raw_terminal_write_paths = BTreeSet::new();
        let mut check_commands = BTreeSet::new();
        let mut latest_check_status: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut check_target_paths = BTreeSet::new();
        let mut latest_mutation_error_status: BTreeMap<String, String> = BTreeMap::new();
        let mut unresolved_error_events = 0usize;
        let mut unresolved_stale_paths = BTreeSet::new();
        let mut unresolved_context_miss_paths = BTreeSet::new();
        let mut read_hashes: BTreeMap<(String, String, String), String> = BTreeMap::new();
        let mut read_paths: BTreeSet<(String, String)> = BTreeSet::new();

        for event in events {
            sessions.insert(event.session_id.clone());
            agents.insert(event.agent_id.clone());
            if let Some(path) = &event.path {
                paths.insert(path.clone());
            }
            if let Some(symbol) = &event.symbol {
                symbols.insert(symbol.clone());
            }
            if event.status == "error" && event.event_type != "check.run" {
                summary.errors += 1;
                if !matches!(event.event_type.as_str(), "edit.apply" | "write.apply")
                    || event.path.is_none()
                {
                    unresolved_error_events += 1;
                }
            }
            match (event.event_type.as_str(), event.status.as_str()) {
                ("context.read", "ok") => {
                    summary.context_reads += 1;
                    if let (Some(path), Some(source_hash)) =
                        (&event.path, event.meta.get("source_hash"))
                    {
                        read_paths.insert((event.session_id.clone(), path.clone()));
                        unresolved_context_miss_paths.remove(path);
                        read_hashes.insert(
                            (
                                event.session_id.clone(),
                                event.agent_id.clone(),
                                path.clone(),
                            ),
                            source_hash.clone(),
                        );
                    }
                }
                ("context.read", "cached") => {
                    summary.context_reads += 1;
                    summary.cache_hits += 1;
                    if let (Some(path), Some(source_hash)) =
                        (&event.path, event.meta.get("source_hash"))
                    {
                        read_paths.insert((event.session_id.clone(), path.clone()));
                        unresolved_context_miss_paths.remove(path);
                        read_hashes.insert(
                            (
                                event.session_id.clone(),
                                event.agent_id.clone(),
                                path.clone(),
                            ),
                            source_hash.clone(),
                        );
                    }
                }
                ("edit.apply", "ok") => {
                    summary.edits_ok += 1;
                    if let Some(path) = &event.path {
                        recorded_write_paths.insert(path.clone());
                        latest_mutation_error_status.insert(path.clone(), "ok".to_string());
                        unresolved_stale_paths.remove(path);
                    }
                    summary.record_write_quality(
                        event,
                        &read_paths,
                        &read_hashes,
                        &mut unresolved_context_miss_paths,
                        &mut risky_paths,
                    );
                }
                ("write.apply", "ok") => {
                    summary.writes_ok += 1;
                    if let Some(path) = &event.path {
                        recorded_write_paths.insert(path.clone());
                        latest_mutation_error_status.insert(path.clone(), "ok".to_string());
                        unresolved_stale_paths.remove(path);
                    }
                    summary.record_write_quality(
                        event,
                        &read_paths,
                        &read_hashes,
                        &mut unresolved_context_miss_paths,
                        &mut risky_paths,
                    );
                }
                ("check.run", "ok") => {
                    summary.checks_ok += 1;
                    summary.record_check_quality(
                        event,
                        &mut check_commands,
                        &mut latest_check_status,
                        &mut check_target_paths,
                    );
                    if event.meta.get("check_kind").map(String::as_str) == Some("test") {
                        summary.test_checks_ok += 1;
                    }
                }
                ("check.run", "error") => {
                    summary.checks_failed += 1;
                    summary.record_check_quality(
                        event,
                        &mut check_commands,
                        &mut latest_check_status,
                        &mut check_target_paths,
                    );
                    if event.meta.get("check_kind").map(String::as_str) == Some("test") {
                        summary.test_checks_failed += 1;
                    }
                }
                ("edit.apply", "error") | ("write.apply", "error") => {
                    if let Some(path) = &event.path {
                        latest_mutation_error_status.insert(path.clone(), "error".to_string());
                    }
                }
                ("lock.acquire", "blocked") => summary.lock_blocks += 1,
                ("edit.guard", "blocked") | ("write.guard", "blocked") => {
                    summary.stale_write_blocks += 1;
                    if let Some(path) = &event.path {
                        unresolved_stale_paths.insert(path.clone());
                    }
                }
                ("terminal.raw_write", "error") => {
                    summary.raw_terminal_writes += 1;
                    if let Some(path) = &event.path {
                        raw_terminal_write_paths.insert(path.clone());
                    }
                }
                _ => {}
            }
        }

        summary.recorded_write_paths = recorded_write_paths.into_iter().collect();
        summary.refresh_changed_scope();
        summary.paths = paths.into_iter().collect();
        summary.symbols = symbols.into_iter().collect();
        summary.sessions = sessions.into_iter().collect();
        summary.agents = agents.into_iter().collect();
        summary.risky_paths = risky_paths.into_iter().collect();
        summary.raw_terminal_write_paths = raw_terminal_write_paths.into_iter().collect();
        summary.check_commands = check_commands.into_iter().collect();
        let unresolved: Vec<(String, String)> = latest_check_status
            .into_iter()
            .filter_map(|(command, (status, kind))| {
                if status == "error" {
                    Some((command, kind))
                } else {
                    None
                }
            })
            .collect();
        summary.unresolved_checks_failed = unresolved.len();
        summary.unresolved_test_checks_failed =
            unresolved.iter().filter(|(_, kind)| kind == "test").count();
        summary.unresolved_check_commands =
            unresolved.into_iter().map(|(command, _)| command).collect();
        summary.check_target_paths = check_target_paths.into_iter().collect();
        summary.unresolved_errors = unresolved_error_events
            + latest_mutation_error_status
                .values()
                .filter(|status| status.as_str() == "error")
                .count();
        summary.unresolved_stale_write_blocks = unresolved_stale_paths.len();
        summary.unresolved_stale_write_paths = unresolved_stale_paths.into_iter().collect();
        summary.unresolved_edits_without_file_context = unresolved_context_miss_paths.len();
        summary.unresolved_edits_without_file_context_paths =
            unresolved_context_miss_paths.into_iter().collect();
        summary
    }

    pub fn with_unrecorded_repo_changes(mut self, changed_paths: Vec<String>) -> Self {
        let recorded_writes: BTreeSet<String> = self
            .recorded_write_paths
            .iter()
            .filter(|path| !path.starts_with(".anchor/"))
            .cloned()
            .collect();
        let unrecorded: Vec<String> = changed_paths
            .into_iter()
            .filter(|path| !recorded_writes.contains(path))
            .collect();
        self.unrecorded_changed_files = unrecorded.len();
        self.unrecorded_changed_file_list = unrecorded;
        self.refresh_changed_scope();
        self
    }

    pub fn quality_profile(&self) -> QualityProfile {
        let mut score: i32 = 100;
        let mut flags = Vec::new();
        let mut recommendations = BTreeSet::new();
        let changed = self.edits_ok + self.writes_ok > 0;
        let checked = self.checks_ok + self.checks_failed > 0;

        if changed && self.context_reads == 0 {
            score -= 30;
            flags.push("changed_without_recorded_context".to_string());
            recommendations.insert(
                "read relevant context through anchor context/task before editing".to_string(),
            );
        }
        if self.unresolved_edits_without_file_context > 0 {
            score -= 20;
            flags.push("edited_file_without_prior_context".to_string());
            recommendations
                .insert("reread each edited file through Anchor before continuing".to_string());
        }
        if changed && !checked {
            score -= 25;
            flags.push("changed_without_recorded_check".to_string());
            recommendations
                .insert("run a relevant verification command through anchor check".to_string());
        }
        if changed && checked && self.test_checks_ok + self.test_checks_failed == 0 {
            score -= 15;
            flags.push("changed_without_test_check".to_string());
            recommendations.insert(
                "run at least one focused test-like command through anchor check before handoff"
                    .to_string(),
            );
        }
        if self.unresolved_checks_failed > 0 {
            score -= 30;
            flags.push("unresolved_failed_check".to_string());
            recommendations.insert("fix or rerun failing checks before handoff".to_string());
        }
        if self.unresolved_errors > 0 {
            score -= 20;
            flags.push("execution_error".to_string());
            recommendations.insert("resolve recorded execution errors before handoff".to_string());
        }
        if self.lock_blocks > 0 {
            score -= 5;
            flags.push("lock_conflict_seen".to_string());
            recommendations
                .insert("coordinate ownership before editing blocked symbols/files".to_string());
        }
        if self.unresolved_stale_write_blocks > 0 {
            score -= 10;
            flags.push("stale_write_blocked".to_string());
            recommendations.insert("reread stale files and retry from fresh context".to_string());
        }
        if self.changed_scope_paths().len() > 3 {
            score -= 10;
            flags.push("broad_file_scope".to_string());
            recommendations
                .insert("reduce patch scope or split the work into smaller tasks".to_string());
        }
        if self.oversized_edits > 0 {
            score -= 15;
            flags.push("oversized_edit_scope".to_string());
            recommendations
                .insert("review large changed ranges and split unrelated edits".to_string());
        }
        if self.raw_terminal_writes > 0 {
            score -= 25;
            flags.push("raw_terminal_write".to_string());
            recommendations.insert(
                "rerun mutating terminal work through Anchor-controlled writes".to_string(),
            );
        }
        if self.unrecorded_changed_files > 0 {
            score -= 25;
            flags.push("unrecorded_repo_changes".to_string());
            recommendations.insert(
                "route changed files through anchor edit/write or inspect raw terminal writes"
                    .to_string(),
            );
        }
        if !self.risky_paths.is_empty() && checked == false {
            score -= 10;
            flags.push("risky_path_changed_without_check".to_string());
            recommendations.insert(
                "run focused checks for risky files such as auth, billing, config, or migrations"
                    .to_string(),
            );
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

        QualityProfile {
            score,
            risk,
            flags,
            recommendations: recommendations.into_iter().collect(),
        }
    }

    fn record_write_quality(
        &mut self,
        event: &ExecutionEvent,
        read_paths: &BTreeSet<(String, String)>,
        read_hashes: &BTreeMap<(String, String, String), String>,
        unresolved_context_miss_paths: &mut BTreeSet<String>,
        risky_paths: &mut BTreeSet<String>,
    ) {
        let Some(path) = &event.path else {
            return;
        };

        if event.meta.get("expected_hash_source").map(String::as_str) != Some("none") {
            self.guarded_writes += 1;
        }

        let exact_key = (
            event.session_id.clone(),
            event.agent_id.clone(),
            path.clone(),
        );
        let session_key = (event.session_id.clone(), path.clone());
        if !read_hashes.contains_key(&exact_key) && !read_paths.contains(&session_key) {
            self.edits_without_file_context += 1;
            unresolved_context_miss_paths.insert(path.clone());
        }

        if let Some(new_changed_lines) = event
            .meta
            .get("new_changed_lines")
            .and_then(|value| value.parse::<usize>().ok())
        {
            self.changed_line_total += new_changed_lines;
            self.max_changed_lines = self.max_changed_lines.max(new_changed_lines);
            if new_changed_lines > 150 {
                self.oversized_edits += 1;
            }
        }

        if is_risky_path(path) {
            risky_paths.insert(path.clone());
        }
    }

    fn record_check_quality(
        &mut self,
        event: &ExecutionEvent,
        check_commands: &mut BTreeSet<String>,
        latest_check_status: &mut BTreeMap<String, (String, String)>,
        check_target_paths: &mut BTreeSet<String>,
    ) {
        let command = event
            .meta
            .get("command")
            .cloned()
            .or_else(|| {
                event.message.as_ref().and_then(|message| {
                    message
                        .strip_prefix("exit=")
                        .and_then(|msg| msg.split_once(" cmd=").map(|(_, cmd)| cmd.to_string()))
                })
            })
            .unwrap_or_else(|| "<unknown>".to_string());
        let kind = event
            .meta
            .get("check_kind")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        check_commands.insert(command.clone());
        latest_check_status.insert(command, (event.status.clone(), kind));

        if let Some(targets) = event.meta.get("target_paths") {
            for path in targets
                .lines()
                .map(str::trim)
                .filter(|path| !path.is_empty())
            {
                check_target_paths.insert(path.to_string());
            }
        }
    }

    fn changed_scope_paths(&self) -> BTreeSet<String> {
        self.recorded_write_paths
            .iter()
            .chain(self.unrecorded_changed_file_list.iter())
            .filter(|path| !path.starts_with(".anchor/"))
            .cloned()
            .collect()
    }

    fn refresh_changed_scope(&mut self) {
        self.changed_file_scope_paths = self.changed_scope_paths().into_iter().collect();
        self.changed_file_scope = self.changed_file_scope_paths.len();
    }
}

fn is_risky_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains("auth")
        || lower.contains("login")
        || lower.contains("permission")
        || lower.contains("security")
        || lower.contains("secret")
        || lower.contains("payment")
        || lower.contains("billing")
        || lower.contains("refund")
        || lower.contains("migration")
        || lower.contains("schema")
        || lower.ends_with(".env")
        || lower.contains("config")
}

impl ExecutionEvent {
    pub fn new(
        event_type: impl Into<String>,
        path: Option<String>,
        symbol: Option<String>,
        status: impl Into<String>,
        message: Option<String>,
    ) -> Self {
        Self::new_with_meta(event_type, path, symbol, status, message, BTreeMap::new())
    }

    pub fn new_with_meta(
        event_type: impl Into<String>,
        path: Option<String>,
        symbol: Option<String>,
        status: impl Into<String>,
        message: Option<String>,
        meta: BTreeMap<String, String>,
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
            meta,
        }
    }
}

/// Rotate the log once it crosses this size so per-edit costs stay bounded on
/// long sessions. Rotated segments keep full provenance on disk.
const MAX_LOG_BYTES: u64 = 16 * 1024 * 1024;

pub fn append(anchor_root: &Path, event: &ExecutionEvent) -> anyhow::Result<()> {
    let dir = anchor_root.join("events");
    fs::create_dir_all(&dir)?;
    let path = dir.join("events.jsonl");
    let _guard = EventLogLock::acquire(&dir)?;
    rotate_if_oversized(&dir, &path, event.timestamp_ms)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = format!("{}\n", serde_json::to_string(event)?);
    file.write_all(line.as_bytes())?;
    update_read_hash_index(&dir, event);
    Ok(())
}

fn rotate_if_oversized(dir: &Path, path: &Path, timestamp_ms: u128) -> anyhow::Result<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() < MAX_LOG_BYTES {
        return Ok(());
    }
    let rotated = dir.join(format!("events-{}.jsonl", timestamp_ms));
    fs::rename(path, rotated)?;
    Ok(())
}

/// Compact index of the last source hash each (session, agent) read per path.
/// Lets the write guard answer "what did this agent last read?" without
/// re-scanning the whole event log on every edit.
fn read_hash_index_path(dir: &Path) -> PathBuf {
    dir.join("read_hashes.json")
}

fn read_hash_key(session_id: &str, agent_id: &str, path: &str) -> String {
    format!("{}\u{1f}{}\u{1f}{}", session_id, agent_id, path)
}

fn update_read_hash_index(dir: &Path, event: &ExecutionEvent) {
    if event.event_type != "context.read" || !matches!(event.status.as_str(), "ok" | "cached") {
        return;
    }
    let (Some(path), Some(source_hash)) = (&event.path, event.meta.get("source_hash")) else {
        return;
    };
    let index_path = read_hash_index_path(dir);
    let mut index: BTreeMap<String, String> = fs::read_to_string(&index_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    index.insert(
        read_hash_key(&event.session_id, &event.agent_id, path),
        source_hash.clone(),
    );
    if let Ok(raw) = serde_json::to_vec(&index) {
        if let Err(err) = fs::write(&index_path, raw) {
            eprintln!(
                "anchor: failed to update read-hash index {}: {err}",
                index_path.display()
            );
        }
    }
}

/// Last source hash a (session, agent) read for `repo_path`, served from the
/// compact index. Falls back to a full log scan only when the index file does
/// not exist yet (logs written by older Anchor versions).
pub fn last_read_hash(
    anchor_root: &Path,
    session_id: &str,
    agent_id: &str,
    repo_path: &str,
) -> Option<String> {
    let dir = anchor_root.join("events");
    let index_path = read_hash_index_path(&dir);
    if index_path.exists() {
        let index: BTreeMap<String, String> =
            serde_json::from_str(&fs::read_to_string(&index_path).ok()?).ok()?;
        return index
            .get(&read_hash_key(session_id, agent_id, repo_path))
            .cloned();
    }

    let events = load(anchor_root).ok()?;
    events
        .iter()
        .rev()
        .find(|event| {
            event.event_type == "context.read"
                && matches!(event.status.as_str(), "ok" | "cached")
                && event.path.as_deref() == Some(repo_path)
                && event.session_id == session_id
                && event.agent_id == agent_id
        })
        .and_then(|event| event.meta.get("source_hash").cloned())
}

struct EventLogLock {
    path: PathBuf,
}

impl EventLogLock {
    fn acquire(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join("events.lock");
        for _ in 0..500 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => return Err(err.into()),
            }
        }
        anyhow::bail!(
            "timed out waiting for Anchor event log lock: {}",
            path.display()
        )
    }
}

impl Drop for EventLogLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
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

    let mut corrupt_lines = 0usize;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(event) => events.push(event),
            // One torn or corrupted line must not make the whole log — and
            // with it the write guard — unreadable.
            Err(_) => corrupt_lines += 1,
        }
    }
    if corrupt_lines > 0 {
        eprintln!("anchor: skipped {corrupt_lines} corrupt event log line(s)");
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
    if let Err(err) = append(anchor_root, &event) {
        eprintln!("anchor: failed to record event: {err}");
    }
}

/// Record an event that the caller treats as load-bearing: mutating
/// operations call this *before* touching the file so that "no receipt, no
/// write" holds — a flight recorder that can silently fail is not evidence.
pub fn record_required(
    anchor_root: &Path,
    event_type: impl Into<String>,
    path: Option<String>,
    symbol: Option<String>,
    status: impl Into<String>,
    message: Option<String>,
) -> anyhow::Result<()> {
    let event = ExecutionEvent::new(event_type, path, symbol, status, message);
    append(anchor_root, &event)
}

pub fn record_with_meta(
    anchor_root: &Path,
    event_type: impl Into<String>,
    path: Option<String>,
    symbol: Option<String>,
    status: impl Into<String>,
    message: Option<String>,
    meta: BTreeMap<String, String>,
) {
    let event = ExecutionEvent::new_with_meta(event_type, path, symbol, status, message, meta);
    if let Err(err) = append(anchor_root, &event) {
        eprintln!("anchor: failed to record event: {err}");
    }
}

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

include!("events_parts/summary_from.rs");
include!("events_parts/summary_quality.rs");
include!("events_parts/summary_record.rs");
include!("events_parts/event_construct.rs");
include!("events_parts/read_hash.rs");
include!("events_parts/log_io.rs");
include!("events_parts/record.rs");

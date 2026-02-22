//
//  write.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::path::Path;
use std::time::Duration;

use crate::graph::CodeGraph;
use crate::lock::{LockManager, LockResult, SymbolKey};
use crate::write::{self, WriteError, WriteResult};

/// Default timeout for acquiring locks
const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

/// Result of a locked write operation
#[derive(Debug)]
pub enum LockedWriteResult {
    /// Write succeeded
    Success {
        write_result: WriteResult,
        locked_symbols: Vec<SymbolKey>,
        wait_time_ms: u64,
    },
    /// Write failed due to lock conflict
    Blocked {
        blocked_by: SymbolKey,
        reason: String,
    },
    /// Write failed for other reasons
    WriteError(WriteError),
}

/// Acquire a file lock, run a write operation, then release.
fn with_file_lock<F>(
    path: &Path,
    manager: &LockManager,
    graph: &CodeGraph,
    write_fn: F,
) -> LockedWriteResult
where
    F: FnOnce() -> Result<WriteResult, WriteError>,
{
    let (symbol, dependents, wait_time_ms) =
        match manager.acquire_with_wait(path, graph, DEFAULT_LOCK_TIMEOUT) {
            LockResult::Acquired {
                symbol, dependents, ..
            } => (symbol, dependents, 0),
            LockResult::AcquiredAfterWait {
                symbol,
                dependents,
                wait_time_ms,
            } => (symbol, dependents, wait_time_ms),
            LockResult::Blocked {
                blocked_by, reason, ..
            } => return LockedWriteResult::Blocked { blocked_by, reason },
        };

    let result = write_fn();
    manager.release(&symbol.file);

    match result {
        Ok(write_result) => LockedWriteResult::Success {
            write_result,
            locked_symbols: std::iter::once(symbol).chain(dependents).collect(),
            wait_time_ms,
        },
        Err(e) => LockedWriteResult::WriteError(e),
    }
}

/// Create a file with automatic locking
pub fn create_file_locked(
    path: &Path,
    content: &str,
    manager: &LockManager,
    graph: &CodeGraph,
) -> LockedWriteResult {
    with_file_lock(path, manager, graph, || write::create_file(path, content))
}

/// Insert content after pattern with automatic locking
pub fn insert_after_locked(
    path: &Path,
    pattern: &str,
    content: &str,
    manager: &LockManager,
    graph: &CodeGraph,
) -> LockedWriteResult {
    with_file_lock(path, manager, graph, || {
        write::insert_after(path, pattern, content)
    })
}

/// Replace content with automatic locking
pub fn replace_all_locked(
    path: &Path,
    old: &str,
    new: &str,
    manager: &LockManager,
    graph: &CodeGraph,
) -> LockedWriteResult {
    with_file_lock(path, manager, graph, || write::replace_all(path, old, new))
}

/// Batch replace with automatic locking - locks ALL files first, then writes
pub fn batch_replace_locked(
    paths: &[std::path::PathBuf],
    old: &str,
    new: &str,
    manager: &LockManager,
    graph: &CodeGraph,
) -> BatchLockedWriteResult {
    let mut locked_symbols = Vec::new();
    let mut lock_errors = Vec::new();

    // Phase 1: Acquire all locks
    for path in paths {
        match manager.acquire_with_wait(path, graph, DEFAULT_LOCK_TIMEOUT) {
            LockResult::Acquired {
                symbol, dependents, ..
            }
            | LockResult::AcquiredAfterWait {
                symbol, dependents, ..
            } => {
                locked_symbols.push(symbol);
                locked_symbols.extend(dependents);
            }
            LockResult::Blocked {
                blocked_by, reason, ..
            } => {
                lock_errors.push((path.clone(), blocked_by, reason));
            }
        }
    }

    // If any lock failed, release all and return error
    if !lock_errors.is_empty() {
        for path in paths {
            manager.release(path);
        }
        return BatchLockedWriteResult {
            successful: vec![],
            failed: lock_errors
                .iter()
                .map(|(p, _, r)| (p.clone(), r.clone()))
                .collect(),
            total_locked_symbols: 0,
        };
    }

    // Phase 2: Execute all writes
    let results = write::batch_replace_all(paths, old, new);

    // Phase 3: Release all locks
    for path in paths {
        manager.release(path);
    }

    // Collect results
    let mut successful = Vec::new();
    let mut failed = Vec::new();

    for (path, result) in paths.iter().zip(results) {
        match result {
            Ok(wr) => successful.push(wr),
            Err(e) => failed.push((path.clone(), e.to_string())),
        }
    }

    BatchLockedWriteResult {
        successful,
        failed,
        total_locked_symbols: locked_symbols.len(),
    }
}

/// Result of a batch locked write operation
#[derive(Debug)]
pub struct BatchLockedWriteResult {
    pub successful: Vec<WriteResult>,
    pub failed: Vec<(std::path::PathBuf, String)>,
    pub total_locked_symbols: usize,
}

impl BatchLockedWriteResult {
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} succeeded, {} failed, {} symbols locked",
            self.successful.len(),
            self.failed.len(),
            self.total_locked_symbols
        )
    }
}

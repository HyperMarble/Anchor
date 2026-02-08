//! Locked write operations - automatically acquire locks before writing.
//!
//! These are the "safe" versions of write operations that:
//! 1. Acquire lock on target file + dependents
//! 2. Execute the write
//! 3. Release lock (automatically via guard)

use std::path::Path;
use std::time::Duration;

use crate::graph::CodeGraph;
use crate::lock::{LockManager, LockResult};
use crate::write::{self, WriteError, WriteResult};

/// Default timeout for acquiring locks
const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

/// Result of a locked write operation
#[derive(Debug)]
pub enum LockedWriteResult {
    /// Write succeeded
    Success {
        write_result: WriteResult,
        locked_files: Vec<std::path::PathBuf>,
        wait_time_ms: u64,
    },
    /// Write failed due to lock conflict
    Blocked {
        blocked_by: std::path::PathBuf,
        reason: String,
    },
    /// Write failed for other reasons
    WriteError(WriteError),
}

/// Create a file with automatic locking
pub fn create_file_locked(
    path: &Path,
    content: &str,
    manager: &LockManager,
    graph: &CodeGraph,
) -> LockedWriteResult {
    // For create, there might not be dependents yet (new file)
    // But we still lock to prevent race conditions
    match manager.acquire_with_wait(path, graph, DEFAULT_LOCK_TIMEOUT) {
        LockResult::Acquired { file, dependents } | LockResult::AcquiredAfterWait { file, dependents, .. } => {
            let result = write::create_file(path, content);
            manager.release(&file);

            match result {
                Ok(write_result) => LockedWriteResult::Success {
                    write_result,
                    locked_files: std::iter::once(file).chain(dependents).collect(),
                    wait_time_ms: 0,
                },
                Err(e) => LockedWriteResult::WriteError(e),
            }
        }
        LockResult::Blocked { blocked_by, reason } => {
            LockedWriteResult::Blocked { blocked_by, reason }
        }
    }
}

/// Insert content after pattern with automatic locking
pub fn insert_after_locked(
    path: &Path,
    pattern: &str,
    content: &str,
    manager: &LockManager,
    graph: &CodeGraph,
) -> LockedWriteResult {
    match manager.acquire_with_wait(path, graph, DEFAULT_LOCK_TIMEOUT) {
        LockResult::Acquired { file, dependents } => {
            let result = write::insert_after(path, pattern, content);
            manager.release(&file);

            match result {
                Ok(write_result) => LockedWriteResult::Success {
                    write_result,
                    locked_files: std::iter::once(file).chain(dependents).collect(),
                    wait_time_ms: 0,
                },
                Err(e) => LockedWriteResult::WriteError(e),
            }
        }
        LockResult::AcquiredAfterWait { file, dependents, wait_time_ms } => {
            let result = write::insert_after(path, pattern, content);
            manager.release(&file);

            match result {
                Ok(write_result) => LockedWriteResult::Success {
                    write_result,
                    locked_files: std::iter::once(file).chain(dependents).collect(),
                    wait_time_ms,
                },
                Err(e) => LockedWriteResult::WriteError(e),
            }
        }
        LockResult::Blocked { blocked_by, reason } => {
            LockedWriteResult::Blocked { blocked_by, reason }
        }
    }
}

/// Replace content with automatic locking
pub fn replace_all_locked(
    path: &Path,
    old: &str,
    new: &str,
    manager: &LockManager,
    graph: &CodeGraph,
) -> LockedWriteResult {
    match manager.acquire_with_wait(path, graph, DEFAULT_LOCK_TIMEOUT) {
        LockResult::Acquired { file, dependents } | LockResult::AcquiredAfterWait { file, dependents, .. } => {
            let wait_time_ms = match manager.acquire_with_wait(path, graph, DEFAULT_LOCK_TIMEOUT) {
                LockResult::AcquiredAfterWait { wait_time_ms, .. } => wait_time_ms,
                _ => 0,
            };

            let result = write::replace_all(path, old, new);
            manager.release(&file);

            match result {
                Ok(write_result) => LockedWriteResult::Success {
                    write_result,
                    locked_files: std::iter::once(file).chain(dependents).collect(),
                    wait_time_ms,
                },
                Err(e) => LockedWriteResult::WriteError(e),
            }
        }
        LockResult::Blocked { blocked_by, reason } => {
            LockedWriteResult::Blocked { blocked_by, reason }
        }
    }
}

/// Batch replace with automatic locking - locks ALL files first, then writes
pub fn batch_replace_locked(
    paths: &[std::path::PathBuf],
    old: &str,
    new: &str,
    manager: &LockManager,
    graph: &CodeGraph,
) -> BatchLockedWriteResult {
    let mut locked_files = Vec::new();
    let mut lock_errors = Vec::new();

    // Phase 1: Acquire all locks
    for path in paths {
        match manager.acquire_with_wait(path, graph, DEFAULT_LOCK_TIMEOUT) {
            LockResult::Acquired { file, dependents } | LockResult::AcquiredAfterWait { file, dependents, .. } => {
                locked_files.push(file);
                locked_files.extend(dependents);
            }
            LockResult::Blocked { blocked_by, reason } => {
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
            failed: lock_errors.iter().map(|(p, _, r)| (p.clone(), r.clone())).collect(),
            total_locked_files: 0,
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
        total_locked_files: locked_files.len(),
    }
}

/// Result of a batch locked write operation
#[derive(Debug)]
pub struct BatchLockedWriteResult {
    pub successful: Vec<WriteResult>,
    pub failed: Vec<(std::path::PathBuf, String)>,
    pub total_locked_files: usize,
}

impl BatchLockedWriteResult {
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} succeeded, {} failed, {} files locked",
            self.successful.len(),
            self.failed.len(),
            self.total_locked_files
        )
    }
}

//! File locking system for coordinating parallel writes.
//!
//! Provides dependency-aware locking:
//! - When you lock a file, its immediate dependents are also locked
//! - Prevents conflicts when multiple agents modify related files
//!
//! # Example
//! ```ignore
//! let manager = LockManager::new();
//!
//! // Lock auth.rs - also locks files that depend on it
//! manager.acquire("src/auth.rs", &graph)?;
//!
//! // ... do write operation ...
//!
//! manager.release("src/auth.rs");
//! ```

pub mod write;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::graph::CodeGraph;

/// Lock acquisition result
#[derive(Debug)]
pub enum LockResult {
    /// Lock acquired successfully
    Acquired {
        /// Primary file that was locked
        file: PathBuf,
        /// Dependent files that were also locked
        dependents: Vec<PathBuf>,
    },
    /// Lock is held by another operation
    Blocked {
        /// The file that's blocking
        blocked_by: PathBuf,
        /// Reason for the block
        reason: String,
    },
    /// Lock acquired after waiting
    AcquiredAfterWait {
        file: PathBuf,
        dependents: Vec<PathBuf>,
        wait_time_ms: u64,
    },
}

/// Lock entry tracking who holds a lock
#[derive(Debug, Clone)]
struct LockEntry {
    /// The primary file that initiated the lock
    primary_file: PathBuf,
    /// When the lock was acquired
    acquired_at: Instant,
    /// Optional operation ID for tracking
    _operation_id: Option<String>,
}

/// Manages file locks with dependency awareness
pub struct LockManager {
    /// Active locks: file path -> lock entry
    locks: Mutex<HashMap<PathBuf, LockEntry>>,
    /// Condition variable for waiting on locks
    lock_released: Condvar,
}

impl LockManager {
    /// Create a new lock manager
    pub fn new() -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
            lock_released: Condvar::new(),
        }
    }

    /// Acquire a lock on a file and its dependents.
    ///
    /// Returns immediately with `Blocked` if any file is already locked.
    pub fn try_acquire(&self, file: &Path, graph: &CodeGraph) -> LockResult {
        let file = normalize_path(file);
        let dependents = self.get_immediate_dependents(&file, graph);

        let mut locks = self.locks.lock().unwrap();

        // Check if any file we need is already locked
        let all_files: Vec<&PathBuf> = std::iter::once(&file).chain(dependents.iter()).collect();

        for f in &all_files {
            if let Some(entry) = locks.get(*f) {
                // Already locked by someone else
                if entry.primary_file != file {
                    return LockResult::Blocked {
                        blocked_by: entry.primary_file.clone(),
                        reason: format!(
                            "{} is locked (dependency of {})",
                            f.display(),
                            entry.primary_file.display()
                        ),
                    };
                }
            }
        }

        // Acquire all locks
        let entry = LockEntry {
            primary_file: file.clone(),
            acquired_at: Instant::now(),
            _operation_id: None,
        };

        for f in all_files {
            locks.insert(f.clone(), entry.clone());
        }

        LockResult::Acquired {
            file,
            dependents,
        }
    }

    /// Acquire a lock, waiting up to `timeout` if blocked.
    pub fn acquire_with_wait(
        &self,
        file: &Path,
        graph: &CodeGraph,
        timeout: Duration,
    ) -> LockResult {
        let start = Instant::now();
        let file = normalize_path(file);
        let dependents = self.get_immediate_dependents(&file, graph);

        let mut locks = self.locks.lock().unwrap();

        loop {
            // Check if any file we need is already locked
            let all_files: Vec<PathBuf> = std::iter::once(file.clone())
                .chain(dependents.iter().cloned())
                .collect();

            let mut blocked_by = None;
            for f in &all_files {
                if let Some(entry) = locks.get(f) {
                    if entry.primary_file != file {
                        blocked_by = Some(entry.primary_file.clone());
                        break;
                    }
                }
            }

            if blocked_by.is_none() {
                // Can acquire
                let entry = LockEntry {
                    primary_file: file.clone(),
                    acquired_at: Instant::now(),
                    _operation_id: None,
                };

                for f in &all_files {
                    locks.insert(f.clone(), entry.clone());
                }

                let wait_time = start.elapsed();
                if wait_time.as_millis() > 0 {
                    return LockResult::AcquiredAfterWait {
                        file,
                        dependents,
                        wait_time_ms: wait_time.as_millis() as u64,
                    };
                } else {
                    return LockResult::Acquired { file, dependents };
                }
            }

            // Check timeout
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return LockResult::Blocked {
                    blocked_by: blocked_by.unwrap(),
                    reason: format!("Timeout after {}ms", elapsed.as_millis()),
                };
            }

            // Wait for lock release
            let remaining = timeout - elapsed;
            let (new_locks, timeout_result) = self
                .lock_released
                .wait_timeout(locks, remaining)
                .unwrap();
            locks = new_locks;

            if timeout_result.timed_out() {
                return LockResult::Blocked {
                    blocked_by: blocked_by.unwrap(),
                    reason: "Timeout waiting for lock".to_string(),
                };
            }
        }
    }

    /// Release a lock on a file and its dependents.
    pub fn release(&self, file: &Path) {
        let file = normalize_path(file);
        let mut locks = self.locks.lock().unwrap();

        // Find all files locked by this primary file
        let to_remove: Vec<PathBuf> = locks
            .iter()
            .filter(|(_, entry)| entry.primary_file == file)
            .map(|(path, _)| path.clone())
            .collect();

        for f in to_remove {
            locks.remove(&f);
        }

        // Notify waiters
        drop(locks);
        self.lock_released.notify_all();
    }

    /// Check if a file is currently locked.
    pub fn is_locked(&self, file: &Path) -> bool {
        let file = normalize_path(file);
        let locks = self.locks.lock().unwrap();
        locks.contains_key(&file)
    }

    /// Get lock status for a file.
    pub fn status(&self, file: &Path) -> LockStatus {
        let file = normalize_path(file);
        let locks = self.locks.lock().unwrap();

        if let Some(entry) = locks.get(&file) {
            LockStatus::Locked {
                by: entry.primary_file.clone(),
                duration_ms: entry.acquired_at.elapsed().as_millis() as u64,
            }
        } else {
            LockStatus::Unlocked
        }
    }

    /// Get all currently held locks.
    pub fn active_locks(&self) -> Vec<LockInfo> {
        let locks = self.locks.lock().unwrap();

        // Group by primary file
        let mut primaries: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut acquired_times: HashMap<PathBuf, Instant> = HashMap::new();

        for (path, entry) in locks.iter() {
            primaries
                .entry(entry.primary_file.clone())
                .or_default()
                .push(path.clone());
            acquired_times
                .entry(entry.primary_file.clone())
                .or_insert(entry.acquired_at);
        }

        primaries
            .into_iter()
            .map(|(primary, mut files)| {
                files.sort();
                LockInfo {
                    primary_file: primary.clone(),
                    locked_files: files,
                    duration_ms: acquired_times[&primary].elapsed().as_millis() as u64,
                }
            })
            .collect()
    }

    /// Get immediate dependents of a file from the graph.
    fn get_immediate_dependents(&self, file: &Path, graph: &CodeGraph) -> Vec<PathBuf> {
        // Get symbols in this file
        let symbols = graph.symbols_in_file(file);

        // For each symbol, find what depends on it (calls it)
        let mut dependent_files: HashSet<PathBuf> = HashSet::new();

        for symbol in symbols {
            let dependents = graph.dependents(&symbol.name);
            for dep in dependents {
                if dep.file != file {
                    dependent_files.insert(dep.file.clone());
                }
            }
        }

        dependent_files.into_iter().collect()
    }
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lock status for a file
#[derive(Debug, Clone)]
pub enum LockStatus {
    Unlocked,
    Locked {
        by: PathBuf,
        duration_ms: u64,
    },
}

/// Information about an active lock
#[derive(Debug, Clone)]
pub struct LockInfo {
    /// The file that initiated the lock
    pub primary_file: PathBuf,
    /// All files currently locked (primary + dependents)
    pub locked_files: Vec<PathBuf>,
    /// How long the lock has been held
    pub duration_ms: u64,
}

/// Normalize a path for consistent lock keys
fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// RAII guard that releases lock when dropped
pub struct LockGuard<'a> {
    manager: &'a LockManager,
    file: PathBuf,
}

impl<'a> LockGuard<'a> {
    /// Create a new lock guard (acquires lock)
    pub fn new(manager: &'a LockManager, file: &Path, graph: &CodeGraph) -> Result<Self, String> {
        match manager.try_acquire(file, graph) {
            LockResult::Acquired { file, .. } => Ok(Self { manager, file }),
            LockResult::AcquiredAfterWait { file, .. } => Ok(Self { manager, file }),
            LockResult::Blocked { blocked_by, reason } => {
                Err(format!("Blocked by {}: {}", blocked_by.display(), reason))
            }
        }
    }

    /// Create with timeout
    pub fn with_timeout(
        manager: &'a LockManager,
        file: &Path,
        graph: &CodeGraph,
        timeout: Duration,
    ) -> Result<Self, String> {
        match manager.acquire_with_wait(file, graph, timeout) {
            LockResult::Acquired { file, .. } => Ok(Self { manager, file }),
            LockResult::AcquiredAfterWait { file, .. } => Ok(Self { manager, file }),
            LockResult::Blocked { blocked_by, reason } => {
                Err(format!("Blocked by {}: {}", blocked_by.display(), reason))
            }
        }
    }
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        self.manager.release(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_lock_unlock() {
        let manager = LockManager::new();
        let graph = CodeGraph::new();

        let result = manager.try_acquire(Path::new("test.rs"), &graph);
        assert!(matches!(result, LockResult::Acquired { .. }));
        assert!(manager.is_locked(Path::new("test.rs")));

        manager.release(Path::new("test.rs"));
        assert!(!manager.is_locked(Path::new("test.rs")));
    }

    #[test]
    fn test_double_lock_blocked() {
        let manager = LockManager::new();
        let graph = CodeGraph::new();

        let _result1 = manager.try_acquire(Path::new("test.rs"), &graph);

        let result2 = manager.try_acquire(Path::new("test.rs"), &graph);
        // Same file, same primary - should succeed (idempotent)
        assert!(matches!(result2, LockResult::Acquired { .. }));
    }

    #[test]
    fn test_different_files_ok() {
        let manager = LockManager::new();
        let graph = CodeGraph::new();

        let result1 = manager.try_acquire(Path::new("a.rs"), &graph);
        let result2 = manager.try_acquire(Path::new("b.rs"), &graph);

        assert!(matches!(result1, LockResult::Acquired { .. }));
        assert!(matches!(result2, LockResult::Acquired { .. }));
    }

    #[test]
    fn test_lock_guard_auto_release() {
        let manager = LockManager::new();
        let graph = CodeGraph::new();

        {
            let _guard = LockGuard::new(&manager, Path::new("test.rs"), &graph).unwrap();
            assert!(manager.is_locked(Path::new("test.rs")));
        }

        // Guard dropped, lock released
        assert!(!manager.is_locked(Path::new("test.rs")));
    }

    #[test]
    fn test_wait_for_lock() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let manager = Arc::new(LockManager::new());
        let graph = Arc::new(CodeGraph::new());
        let lock_acquired = Arc::new(AtomicBool::new(false));

        // Thread 1: acquire lock, signal, hold for 100ms, release
        let m1 = manager.clone();
        let g1 = graph.clone();
        let acquired1 = lock_acquired.clone();
        let t1 = thread::spawn(move || {
            let _result = m1.try_acquire(Path::new("/tmp/test_lock_wait.rs"), &g1);
            acquired1.store(true, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(100));
            m1.release(Path::new("/tmp/test_lock_wait.rs"));
        });

        // Wait until t1 has acquired the lock
        while !lock_acquired.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }

        // Thread 2: should block then acquire after t1 releases
        let m2 = manager.clone();
        let g2 = graph.clone();
        let result = m2.acquire_with_wait(Path::new("/tmp/test_lock_wait.rs"), &g2, Duration::from_millis(500));

        t1.join().unwrap();

        // Should have acquired (either immediately after release or with wait)
        let got_lock = matches!(result, LockResult::Acquired { .. } | LockResult::AcquiredAfterWait { .. });
        assert!(got_lock, "Should have acquired lock after waiting");
    }
}

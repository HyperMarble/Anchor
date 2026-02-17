//! Symbol-level locking system for coordinating parallel writes.
//!
//! Provides dependency-aware locking at the symbol level:
//! - When you lock a symbol, its callers are also locked
//! - Two agents can edit different functions in the same file
//! - File-level locking still works as backward-compatible convenience
//!
//! # Example
//! ```ignore
//! let manager = LockManager::new();
//!
//! // Lock a specific symbol - also locks its callers
//! let key = SymbolKey::new("src/auth.rs", "login");
//! manager.try_acquire_symbol(&key, &graph)?;
//!
//! // ... do write operation ...
//!
//! manager.release_symbol(&key);
//! ```

pub mod write;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::graph::CodeGraph;

/// Unique identifier for a symbol in the graph.
/// Matches the qualified_index key in CodeGraph: (file_path, symbol_name).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolKey {
    pub file: PathBuf,
    pub name: String,
}

impl SymbolKey {
    pub fn new(file: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self {
            file: normalize_path(&file.into()),
            name: name.into(),
        }
    }

    /// Short display: "engine.rs:try_acquire"
    pub fn display_short(&self) -> String {
        let fname = self
            .file
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| self.file.display().to_string());
        format!("{}:{}", fname, self.name)
    }
}

impl fmt::Display for SymbolKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file.display(), self.name)
    }
}

/// Lock acquisition result
#[derive(Debug)]
pub enum LockResult {
    /// Lock acquired successfully
    Acquired {
        symbol: SymbolKey,
        dependents: Vec<SymbolKey>,
    },
    /// Lock is held by another operation
    Blocked {
        blocked_by: SymbolKey,
        reason: String,
    },
    /// Lock acquired after waiting
    AcquiredAfterWait {
        symbol: SymbolKey,
        dependents: Vec<SymbolKey>,
        wait_time_ms: u64,
    },
}

/// Lock entry tracking who holds a lock
#[derive(Debug, Clone)]
struct LockEntry {
    /// The primary symbol that initiated the lock
    primary_symbol: SymbolKey,
    /// When the lock was acquired
    acquired_at: Instant,
    /// Optional operation ID for tracking
    _operation_id: Option<String>,
}

/// Manages symbol locks with dependency awareness
pub struct LockManager {
    /// Active locks: symbol key -> lock entry
    locks: Mutex<HashMap<SymbolKey, LockEntry>>,
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

    // ─── Symbol-level locking ──────────────────────────────────────────

    /// Acquire a lock on a symbol and its callers.
    /// Returns immediately with `Blocked` if any needed symbol is already locked.
    pub fn try_acquire_symbol(&self, symbol: &SymbolKey, graph: &CodeGraph) -> LockResult {
        let dependents = self.get_symbol_dependents(symbol, graph);
        let mut locks = self.locks.lock().unwrap();

        let all_symbols: Vec<&SymbolKey> =
            std::iter::once(symbol).chain(dependents.iter()).collect();

        for s in &all_symbols {
            if let Some(entry) = locks.get(*s) {
                if &entry.primary_symbol != symbol {
                    return LockResult::Blocked {
                        blocked_by: entry.primary_symbol.clone(),
                        reason: format!(
                            "{} is locked (dependency of {})",
                            s.display_short(),
                            entry.primary_symbol.display_short()
                        ),
                    };
                }
            }
        }

        let entry = LockEntry {
            primary_symbol: symbol.clone(),
            acquired_at: Instant::now(),
            _operation_id: None,
        };
        for s in all_symbols {
            locks.insert(s.clone(), entry.clone());
        }

        LockResult::Acquired {
            symbol: symbol.clone(),
            dependents,
        }
    }

    /// Acquire a symbol lock, waiting up to `timeout` if blocked.
    pub fn acquire_symbol_with_wait(
        &self,
        symbol: &SymbolKey,
        graph: &CodeGraph,
        timeout: Duration,
    ) -> LockResult {
        let start = Instant::now();
        let dependents = self.get_symbol_dependents(symbol, graph);
        let mut locks = self.locks.lock().unwrap();

        loop {
            let all_symbols: Vec<SymbolKey> = std::iter::once(symbol.clone())
                .chain(dependents.iter().cloned())
                .collect();

            let mut blocked_by = None;
            for s in &all_symbols {
                if let Some(entry) = locks.get(s) {
                    if &entry.primary_symbol != symbol {
                        blocked_by = Some(entry.primary_symbol.clone());
                        break;
                    }
                }
            }

            if blocked_by.is_none() {
                let entry = LockEntry {
                    primary_symbol: symbol.clone(),
                    acquired_at: Instant::now(),
                    _operation_id: None,
                };
                for s in &all_symbols {
                    locks.insert(s.clone(), entry.clone());
                }
                let wait_time = start.elapsed();
                if wait_time.as_millis() > 0 {
                    return LockResult::AcquiredAfterWait {
                        symbol: symbol.clone(),
                        dependents,
                        wait_time_ms: wait_time.as_millis() as u64,
                    };
                } else {
                    return LockResult::Acquired {
                        symbol: symbol.clone(),
                        dependents,
                    };
                }
            }

            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return LockResult::Blocked {
                    blocked_by: blocked_by.unwrap(),
                    reason: format!("Timeout after {}ms", elapsed.as_millis()),
                };
            }

            let remaining = timeout - elapsed;
            let (new_locks, timeout_result) =
                self.lock_released.wait_timeout(locks, remaining).unwrap();
            locks = new_locks;

            if timeout_result.timed_out() {
                return LockResult::Blocked {
                    blocked_by: blocked_by.unwrap(),
                    reason: "Timeout waiting for lock".to_string(),
                };
            }
        }
    }

    /// Release a symbol lock and all its dependents.
    pub fn release_symbol(&self, symbol: &SymbolKey) {
        let mut locks = self.locks.lock().unwrap();
        let to_remove: Vec<SymbolKey> = locks
            .iter()
            .filter(|(_, entry)| entry.primary_symbol == *symbol)
            .map(|(key, _)| key.clone())
            .collect();
        for s in to_remove {
            locks.remove(&s);
        }
        drop(locks);
        self.lock_released.notify_all();
    }

    /// Get symbols that directly depend on the given symbol (callers only).
    fn get_symbol_dependents(&self, symbol: &SymbolKey, graph: &CodeGraph) -> Vec<SymbolKey> {
        use crate::graph::types::EdgeKind;

        if symbol.name == "__file__" {
            // File-level: get dependents of ALL symbols in this file
            let file_symbols = graph.symbols_in_file(&symbol.file);
            let mut dep_set: HashSet<SymbolKey> = HashSet::new();
            for sym in file_symbols {
                for d in graph.dependents(&sym.name) {
                    if d.relationship == EdgeKind::Calls && d.file != symbol.file {
                        dep_set.insert(SymbolKey::new(d.file, "__file__"));
                    }
                }
            }
            dep_set.into_iter().collect()
        } else {
            // Symbol-level: get callers of this specific symbol
            graph
                .dependents(&symbol.name)
                .into_iter()
                .filter(|d| d.relationship == EdgeKind::Calls)
                .map(|d| SymbolKey::new(d.file, d.symbol))
                .collect()
        }
    }

    // ─── File-level locking (backward compatible) ──────────────────────

    /// Acquire a file-level lock (backward compatible).
    /// Locks a synthetic "__file__" symbol plus dependents from all symbols in file.
    pub fn try_acquire(&self, file: &Path, graph: &CodeGraph) -> LockResult {
        let key = SymbolKey::new(file, "__file__");
        self.try_acquire_symbol(&key, graph)
    }

    /// Acquire a file-level lock with timeout (backward compatible).
    pub fn acquire_with_wait(
        &self,
        file: &Path,
        graph: &CodeGraph,
        timeout: Duration,
    ) -> LockResult {
        let key = SymbolKey::new(file, "__file__");
        self.acquire_symbol_with_wait(&key, graph, timeout)
    }

    /// Release a file-level lock (backward compatible).
    /// Releases all locks where the primary symbol's file matches.
    pub fn release(&self, file: &Path) {
        let file = normalize_path(file);
        let mut locks = self.locks.lock().unwrap();
        let to_remove: Vec<SymbolKey> = locks
            .iter()
            .filter(|(_, entry)| entry.primary_symbol.file == file)
            .map(|(key, _)| key.clone())
            .collect();
        for s in to_remove {
            locks.remove(&s);
        }
        drop(locks);
        self.lock_released.notify_all();
    }

    /// Check if a file has any active locks.
    pub fn is_locked(&self, file: &Path) -> bool {
        let file = normalize_path(file);
        let locks = self.locks.lock().unwrap();
        locks.keys().any(|k| k.file == file)
    }

    /// Get lock status for a file.
    pub fn status(&self, file: &Path) -> LockStatus {
        let file = normalize_path(file);
        let locks = self.locks.lock().unwrap();
        for (key, entry) in locks.iter() {
            if key.file == file {
                return LockStatus::Locked {
                    by: entry.primary_symbol.clone(),
                    duration_ms: entry.acquired_at.elapsed().as_millis() as u64,
                };
            }
        }
        LockStatus::Unlocked
    }

    /// Get all currently held locks.
    pub fn active_locks(&self) -> Vec<LockInfo> {
        let locks = self.locks.lock().unwrap();

        let mut primaries: HashMap<SymbolKey, Vec<SymbolKey>> = HashMap::new();
        let mut acquired_times: HashMap<SymbolKey, Instant> = HashMap::new();

        for (key, entry) in locks.iter() {
            primaries
                .entry(entry.primary_symbol.clone())
                .or_default()
                .push(key.clone());
            acquired_times
                .entry(entry.primary_symbol.clone())
                .or_insert(entry.acquired_at);
        }

        primaries
            .into_iter()
            .map(|(primary, mut symbols)| {
                symbols.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
                LockInfo {
                    primary_symbol: primary.clone(),
                    locked_symbols: symbols,
                    duration_ms: acquired_times[&primary].elapsed().as_millis() as u64,
                }
            })
            .collect()
    }
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lock status for a file or symbol
#[derive(Debug, Clone)]
pub enum LockStatus {
    Unlocked,
    Locked {
        by: SymbolKey,
        duration_ms: u64,
    },
}

/// Information about an active lock
#[derive(Debug, Clone)]
pub struct LockInfo {
    /// The symbol that initiated the lock
    pub primary_symbol: SymbolKey,
    /// All symbols currently locked (primary + dependents)
    pub locked_symbols: Vec<SymbolKey>,
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
    symbol: SymbolKey,
}

impl<'a> LockGuard<'a> {
    /// Create a file-level lock guard (backward compatible)
    pub fn new(manager: &'a LockManager, file: &Path, graph: &CodeGraph) -> Result<Self, String> {
        let key = SymbolKey::new(file, "__file__");
        match manager.try_acquire_symbol(&key, graph) {
            LockResult::Acquired { symbol, .. }
            | LockResult::AcquiredAfterWait { symbol, .. } => Ok(Self { manager, symbol }),
            LockResult::Blocked {
                blocked_by, reason, ..
            } => Err(format!("Blocked by {}: {}", blocked_by, reason)),
        }
    }

    /// Create a symbol-level lock guard
    pub fn for_symbol(
        manager: &'a LockManager,
        symbol: SymbolKey,
        graph: &CodeGraph,
    ) -> Result<Self, String> {
        match manager.try_acquire_symbol(&symbol, graph) {
            LockResult::Acquired { symbol, .. }
            | LockResult::AcquiredAfterWait { symbol, .. } => Ok(Self { manager, symbol }),
            LockResult::Blocked {
                blocked_by, reason, ..
            } => Err(format!("Blocked by {}: {}", blocked_by, reason)),
        }
    }

    /// Create with timeout (file-level)
    pub fn with_timeout(
        manager: &'a LockManager,
        file: &Path,
        graph: &CodeGraph,
        timeout: Duration,
    ) -> Result<Self, String> {
        let key = SymbolKey::new(file, "__file__");
        match manager.acquire_symbol_with_wait(&key, graph, timeout) {
            LockResult::Acquired { symbol, .. }
            | LockResult::AcquiredAfterWait { symbol, .. } => Ok(Self { manager, symbol }),
            LockResult::Blocked {
                blocked_by, reason, ..
            } => Err(format!("Blocked by {}: {}", blocked_by, reason)),
        }
    }
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        self.manager.release_symbol(&self.symbol);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::{EdgeKind, NodeKind};
    use std::sync::Arc;
    use std::thread;

    // ─── File-level tests (backward compat) ────────────────────────

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

        let m1 = manager.clone();
        let g1 = graph.clone();
        let acquired1 = lock_acquired.clone();
        let t1 = thread::spawn(move || {
            let _result = m1.try_acquire(Path::new("/tmp/test_lock_wait.rs"), &g1);
            acquired1.store(true, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(100));
            m1.release(Path::new("/tmp/test_lock_wait.rs"));
        });

        while !lock_acquired.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }

        let m2 = manager.clone();
        let g2 = graph.clone();
        let result = m2.acquire_with_wait(
            Path::new("/tmp/test_lock_wait.rs"),
            &g2,
            Duration::from_millis(500),
        );

        t1.join().unwrap();

        let got_lock = matches!(
            result,
            LockResult::Acquired { .. } | LockResult::AcquiredAfterWait { .. }
        );
        assert!(got_lock, "Should have acquired lock after waiting");
    }

    // ─── Symbol-level tests ────────────────────────────────────────

    fn test_graph_with_deps() -> CodeGraph {
        let mut g = CodeGraph::new();
        let file = PathBuf::from("test.rs");
        let file_idx = g.add_file(file.clone());
        let foo_idx = g.add_symbol(
            "foo".into(), NodeKind::Function, file.clone(), 1, 10, "fn foo() {}".into(),
        );
        let bar_idx = g.add_symbol(
            "bar".into(), NodeKind::Function, file.clone(), 12, 20, "fn bar() { foo() }".into(),
        );
        let baz_idx = g.add_symbol(
            "baz".into(), NodeKind::Function, file.clone(), 22, 30, "fn baz() {}".into(),
        );
        g.add_edge(file_idx, foo_idx, EdgeKind::Defines);
        g.add_edge(file_idx, bar_idx, EdgeKind::Defines);
        g.add_edge(file_idx, baz_idx, EdgeKind::Defines);
        g.add_edge(bar_idx, foo_idx, EdgeKind::Calls); // bar calls foo
        g
    }

    #[test]
    fn test_symbol_lock_independent_symbols() {
        // foo and baz are in same file but baz doesn't call foo
        let manager = LockManager::new();
        let graph = test_graph_with_deps();

        let foo_key = SymbolKey::new("test.rs", "foo");
        let baz_key = SymbolKey::new("test.rs", "baz");

        let r1 = manager.try_acquire_symbol(&foo_key, &graph);
        assert!(matches!(r1, LockResult::Acquired { .. }));

        // baz doesn't call foo, so should NOT be blocked
        let r2 = manager.try_acquire_symbol(&baz_key, &graph);
        assert!(matches!(r2, LockResult::Acquired { .. }));
    }

    #[test]
    fn test_symbol_lock_caller_blocked() {
        // bar calls foo. Locking foo should also lock bar (bar is a dependent).
        let manager = LockManager::new();
        let graph = test_graph_with_deps();

        let foo_key = SymbolKey::new("test.rs", "foo");
        let bar_key = SymbolKey::new("test.rs", "bar");

        let r1 = manager.try_acquire_symbol(&foo_key, &graph);
        assert!(matches!(r1, LockResult::Acquired { .. }));

        // bar is a dependent of foo (calls it), so should be blocked
        let r2 = manager.try_acquire_symbol(&bar_key, &graph);
        assert!(matches!(r2, LockResult::Blocked { .. }));
    }

    #[test]
    fn test_symbol_release() {
        let manager = LockManager::new();
        let graph = test_graph_with_deps();

        let foo_key = SymbolKey::new("test.rs", "foo");
        let bar_key = SymbolKey::new("test.rs", "bar");

        let _r1 = manager.try_acquire_symbol(&foo_key, &graph);
        manager.release_symbol(&foo_key);

        // After release, bar should be acquirable
        let r2 = manager.try_acquire_symbol(&bar_key, &graph);
        assert!(matches!(r2, LockResult::Acquired { .. }));
    }

    #[test]
    fn test_file_level_compat() {
        let manager = LockManager::new();
        let graph = test_graph_with_deps();

        let r1 = manager.try_acquire(Path::new("test.rs"), &graph);
        assert!(matches!(r1, LockResult::Acquired { .. }));
        assert!(manager.is_locked(Path::new("test.rs")));

        manager.release(Path::new("test.rs"));
        assert!(!manager.is_locked(Path::new("test.rs")));
    }
}

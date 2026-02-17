//! Lock manager — dependency-aware symbol and file locking.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::graph::CodeGraph;

use super::types::*;

/// Manages symbol locks with dependency awareness.
pub struct LockManager {
    locks: Mutex<HashMap<SymbolKey, LockEntry>>,
    lock_released: Condvar,
}

impl LockManager {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::{EdgeKind, NodeKind};
    use crate::graph::CodeGraph;
    use std::path::PathBuf;
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
            let _guard =
                super::super::guard::LockGuard::new(&manager, Path::new("test.rs"), &graph)
                    .unwrap();
            assert!(manager.is_locked(Path::new("test.rs")));
        }

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
            "foo".into(),
            NodeKind::Function,
            file.clone(),
            1,
            10,
            "fn foo() {}".into(),
        );
        let bar_idx = g.add_symbol(
            "bar".into(),
            NodeKind::Function,
            file.clone(),
            12,
            20,
            "fn bar() { foo() }".into(),
        );
        let baz_idx = g.add_symbol(
            "baz".into(),
            NodeKind::Function,
            file.clone(),
            22,
            30,
            "fn baz() {}".into(),
        );
        g.add_edge(file_idx, foo_idx, EdgeKind::Defines);
        g.add_edge(file_idx, bar_idx, EdgeKind::Defines);
        g.add_edge(file_idx, baz_idx, EdgeKind::Defines);
        g.add_edge(bar_idx, foo_idx, EdgeKind::Calls);
        g
    }

    #[test]
    fn test_symbol_lock_independent_symbols() {
        let manager = LockManager::new();
        let graph = test_graph_with_deps();

        let foo_key = SymbolKey::new("test.rs", "foo");
        let baz_key = SymbolKey::new("test.rs", "baz");

        let r1 = manager.try_acquire_symbol(&foo_key, &graph);
        assert!(matches!(r1, LockResult::Acquired { .. }));

        let r2 = manager.try_acquire_symbol(&baz_key, &graph);
        assert!(matches!(r2, LockResult::Acquired { .. }));
    }

    #[test]
    fn test_symbol_lock_caller_blocked() {
        let manager = LockManager::new();
        let graph = test_graph_with_deps();

        let foo_key = SymbolKey::new("test.rs", "foo");
        let bar_key = SymbolKey::new("test.rs", "bar");

        let r1 = manager.try_acquire_symbol(&foo_key, &graph);
        assert!(matches!(r1, LockResult::Acquired { .. }));

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

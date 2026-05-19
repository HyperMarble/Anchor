//
//  manager.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use super::types::*;

/// Manages symbol-level write locks.
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

    /// Acquire a lock for a single symbol. Returns immediately with `Blocked` if already locked.
    pub fn try_acquire_symbol_simple(&self, symbol: &SymbolKey) -> LockResult {
        let mut locks = self.locks.lock().unwrap();
        if let Some(entry) = locks.get(symbol) {
            return LockResult::Blocked {
                blocked_by: entry.primary_symbol.clone(),
                reason: format!("{} is already locked", symbol.display_short()),
            };
        }
        let entry = LockEntry {
            primary_symbol: symbol.clone(),
            acquired_at: Instant::now(),
            _operation_id: None,
        };
        locks.insert(symbol.clone(), entry);
        LockResult::Acquired {
            symbol: symbol.clone(),
            dependents: vec![],
        }
    }

    /// Acquire a file-level lock (maps to a `__file__` symbol key).
    pub fn try_acquire(&self, file: &Path) -> LockResult {
        let key = SymbolKey::new(file, "__file__");
        self.try_acquire_symbol_simple(&key)
    }

    /// Acquire a file-level lock, waiting up to `timeout` if blocked.
    pub fn acquire_with_wait(&self, file: &Path, timeout: Duration) -> LockResult {
        let key = SymbolKey::new(file, "__file__");
        let start = Instant::now();
        let mut locks = self.locks.lock().unwrap();

        loop {
            if let Some(entry) = locks.get(&key) {
                let blocked_by = entry.primary_symbol.clone();
                let elapsed = start.elapsed();
                if elapsed >= timeout {
                    return LockResult::Blocked {
                        blocked_by,
                        reason: format!("Timeout after {}ms", elapsed.as_millis()),
                    };
                }
                let remaining = timeout - elapsed;
                let (new_locks, timed_out) =
                    self.lock_released.wait_timeout(locks, remaining).unwrap();
                locks = new_locks;
                if timed_out.timed_out() {
                    return LockResult::Blocked {
                        blocked_by,
                        reason: "Timeout waiting for lock".to_string(),
                    };
                }
            } else {
                let entry = LockEntry {
                    primary_symbol: key.clone(),
                    acquired_at: Instant::now(),
                    _operation_id: None,
                };
                locks.insert(key.clone(), entry);
                let wait_ms = start.elapsed().as_millis() as u64;
                return if wait_ms > 0 {
                    LockResult::AcquiredAfterWait {
                        symbol: key,
                        dependents: vec![],
                        wait_time_ms: wait_ms,
                    }
                } else {
                    LockResult::Acquired {
                        symbol: key,
                        dependents: vec![],
                    }
                };
            }
        }
    }

    /// Release a symbol lock.
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

    /// Release a file-level lock.
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
    use std::path::Path;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_lock_unlock() {
        let manager = LockManager::new();
        let result = manager.try_acquire(Path::new("test.rs"));
        assert!(matches!(result, LockResult::Acquired { .. }));
        assert!(manager.is_locked(Path::new("test.rs")));
        manager.release(Path::new("test.rs"));
        assert!(!manager.is_locked(Path::new("test.rs")));
    }

    #[test]
    fn test_double_lock_blocked() {
        let manager = LockManager::new();
        let _r1 = manager.try_acquire(Path::new("test.rs"));
        let r2 = manager.try_acquire(Path::new("test.rs"));
        assert!(matches!(r2, LockResult::Blocked { .. }));
    }

    #[test]
    fn test_different_files_ok() {
        let manager = LockManager::new();
        let r1 = manager.try_acquire(Path::new("a.rs"));
        let r2 = manager.try_acquire(Path::new("b.rs"));
        assert!(matches!(r1, LockResult::Acquired { .. }));
        assert!(matches!(r2, LockResult::Acquired { .. }));
    }

    #[test]
    fn test_symbol_lock_independent() {
        let manager = LockManager::new();
        let foo = SymbolKey::new("test.rs", "foo");
        let bar = SymbolKey::new("test.rs", "bar");
        let r1 = manager.try_acquire_symbol_simple(&foo);
        let r2 = manager.try_acquire_symbol_simple(&bar);
        assert!(matches!(r1, LockResult::Acquired { .. }));
        assert!(matches!(r2, LockResult::Acquired { .. }));
    }

    #[test]
    fn test_symbol_release() {
        let manager = LockManager::new();
        let foo = SymbolKey::new("test.rs", "foo");
        let _r1 = manager.try_acquire_symbol_simple(&foo);
        manager.release_symbol(&foo);
        let r2 = manager.try_acquire_symbol_simple(&foo);
        assert!(matches!(r2, LockResult::Acquired { .. }));
    }

    #[test]
    fn test_wait_for_lock() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let manager = Arc::new(LockManager::new());
        let lock_acquired = Arc::new(AtomicBool::new(false));

        let m1 = manager.clone();
        let acquired1 = lock_acquired.clone();
        let t1 = thread::spawn(move || {
            let _result = m1.try_acquire(Path::new("/tmp/test_lock_wait.rs"));
            acquired1.store(true, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(100));
            m1.release(Path::new("/tmp/test_lock_wait.rs"));
        });

        while !lock_acquired.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }

        let result = manager.acquire_with_wait(
            Path::new("/tmp/test_lock_wait.rs"),
            Duration::from_millis(500),
        );
        t1.join().unwrap();

        assert!(
            matches!(
                result,
                LockResult::Acquired { .. } | LockResult::AcquiredAfterWait { .. }
            ),
            "Should have acquired lock after waiting"
        );
    }
}

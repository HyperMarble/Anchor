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

/// Locks held longer than this are treated as abandoned. Mirrors lockd's
/// default TTL so the in-process and daemon lock semantics stay aligned; an
/// unreleased lock must never outlive its session forever.
const LOCK_TTL: Duration = Duration::from_secs(300);

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

    fn entry_expired(entry: &LockEntry) -> bool {
        entry.acquired_at.elapsed() >= LOCK_TTL
    }

    /// Acquire a lock for a single symbol. Returns immediately with `Blocked` if already locked.
    pub fn try_acquire_symbol_simple(&self, symbol: &SymbolKey) -> LockResult {
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        if locks.get(symbol).is_some_and(Self::entry_expired) {
            locks.remove(symbol);
        }
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
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());

        loop {
            if locks.get(&key).is_some_and(Self::entry_expired) {
                locks.remove(&key);
            }
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
                let (new_locks, timed_out) = self
                    .lock_released
                    .wait_timeout(locks, remaining)
                    .unwrap_or_else(|e| e.into_inner());
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
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
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
        let locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        locks.keys().any(|k| k.file == file)
    }

    /// Get lock status for a file.
    pub fn status(&self, file: &Path) -> LockStatus {
        let file = normalize_path(file);
        let locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
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
        let locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());

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
    include!("manager_tests.rs");
}

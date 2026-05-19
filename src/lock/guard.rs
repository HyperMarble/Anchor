//
//  guard.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::path::Path;
use std::time::Duration;

use super::manager::LockManager;
use super::types::{LockResult, SymbolKey};

/// RAII guard that releases a lock when dropped.
pub struct LockGuard<'a> {
    manager: &'a LockManager,
    symbol: SymbolKey,
}

impl<'a> LockGuard<'a> {
    /// Create a file-level lock guard.
    pub fn new(manager: &'a LockManager, file: &Path) -> Result<Self, String> {
        let key = SymbolKey::new(file, "__file__");
        match manager.try_acquire_symbol_simple(&key) {
            LockResult::Acquired { symbol, .. } | LockResult::AcquiredAfterWait { symbol, .. } => {
                Ok(Self { manager, symbol })
            }
            LockResult::Blocked {
                blocked_by, reason, ..
            } => Err(format!("Blocked by {}: {}", blocked_by, reason)),
        }
    }

    /// Create a symbol-level lock guard.
    pub fn for_symbol(manager: &'a LockManager, symbol: SymbolKey) -> Result<Self, String> {
        match manager.try_acquire_symbol_simple(&symbol) {
            LockResult::Acquired { symbol, .. } | LockResult::AcquiredAfterWait { symbol, .. } => {
                Ok(Self { manager, symbol })
            }
            LockResult::Blocked {
                blocked_by, reason, ..
            } => Err(format!("Blocked by {}: {}", blocked_by, reason)),
        }
    }

    /// Create with timeout (file-level).
    pub fn with_timeout(
        manager: &'a LockManager,
        file: &Path,
        timeout: Duration,
    ) -> Result<Self, String> {
        match manager.acquire_with_wait(file, timeout) {
            LockResult::Acquired { symbol, .. } | LockResult::AcquiredAfterWait { symbol, .. } => {
                Ok(Self { manager, symbol })
            }
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

//
//  guard.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::path::Path;
use std::time::Duration;

use crate::graph::CodeGraph;

use super::manager::LockManager;
use super::types::{LockResult, SymbolKey};

/// RAII guard that releases lock when dropped.
pub struct LockGuard<'a> {
    manager: &'a LockManager,
    symbol: SymbolKey,
}

impl<'a> LockGuard<'a> {
    /// Create a file-level lock guard (backward compatible).
    pub fn new(manager: &'a LockManager, file: &Path, graph: &CodeGraph) -> Result<Self, String> {
        let key = SymbolKey::new(file, "__file__");
        match manager.try_acquire_symbol(&key, graph) {
            LockResult::Acquired { symbol, .. } | LockResult::AcquiredAfterWait { symbol, .. } => {
                Ok(Self { manager, symbol })
            }
            LockResult::Blocked {
                blocked_by, reason, ..
            } => Err(format!("Blocked by {}: {}", blocked_by, reason)),
        }
    }

    /// Create a symbol-level lock guard.
    pub fn for_symbol(
        manager: &'a LockManager,
        symbol: SymbolKey,
        graph: &CodeGraph,
    ) -> Result<Self, String> {
        match manager.try_acquire_symbol(&symbol, graph) {
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
        graph: &CodeGraph,
        timeout: Duration,
    ) -> Result<Self, String> {
        let key = SymbolKey::new(file, "__file__");
        match manager.acquire_symbol_with_wait(&key, graph, timeout) {
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

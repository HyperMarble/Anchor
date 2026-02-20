//
//  types.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

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

/// Lock acquisition result.
#[derive(Debug)]
pub enum LockResult {
    /// Lock acquired successfully.
    Acquired {
        symbol: SymbolKey,
        dependents: Vec<SymbolKey>,
    },
    /// Lock is held by another operation.
    Blocked {
        blocked_by: SymbolKey,
        reason: String,
    },
    /// Lock acquired after waiting.
    AcquiredAfterWait {
        symbol: SymbolKey,
        dependents: Vec<SymbolKey>,
        wait_time_ms: u64,
    },
}

/// Lock entry tracking who holds a lock.
#[derive(Debug, Clone)]
pub(crate) struct LockEntry {
    pub primary_symbol: SymbolKey,
    pub acquired_at: Instant,
    pub _operation_id: Option<String>,
}

/// Lock status for a file or symbol.
#[derive(Debug, Clone)]
pub enum LockStatus {
    Unlocked,
    Locked {
        by: SymbolKey,
        duration_ms: u64,
    },
}

/// Information about an active lock.
#[derive(Debug, Clone)]
pub struct LockInfo {
    pub primary_symbol: SymbolKey,
    pub locked_symbols: Vec<SymbolKey>,
    pub duration_ms: u64,
}

/// Normalize a path for consistent lock keys.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

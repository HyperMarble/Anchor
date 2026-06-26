//
//  write.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

thread_local! {
    static GOVERNED_WRITE: Cell<bool> = const { Cell::new(false) };
}

/// Marks the enclosed raw write as coming through the governed path (locks,
/// hash checks, provenance). Direct library calls outside this wrapper are
/// recorded as `write.raw` so bypasses leave evidence just like terminal
/// writes do.
pub(crate) fn governed<T>(operation: impl FnOnce() -> T) -> T {
    GOVERNED_WRITE.with(|flag| {
        let previous = flag.replace(true);
        let result = operation();
        flag.set(previous);
        result
    })
}

fn record_ungoverned_write(path: &Path, operation: &str) {
    if GOVERNED_WRITE.with(|flag| flag.get()) {
        return;
    }
    let Some(parent) = path.parent() else {
        return;
    };
    let Ok(store) = crate::storage::AnchorStore::discover(parent) else {
        return;
    };
    crate::events::record(
        store.anchor_root(),
        "write.raw",
        Some(path.display().to_string()),
        None,
        "warn",
        Some(format!("ungoverned library write: {operation}")),
    );
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Pattern not found: {0}")]
    PatternNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

include!("write_parts/basic.rs");
include!("write_parts/range_result.rs");
include!("write_parts/batch.rs");

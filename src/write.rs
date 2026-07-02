//
//  write.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::fs;
use std::path::{Path, PathBuf};

fn record_ungoverned_write(path: &Path, operation: &str) {
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

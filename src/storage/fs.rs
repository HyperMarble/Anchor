//
//  fs.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Storage layer for Anchor.
pub struct Storage {
    /// Root directory (.anchor/)
    root: PathBuf,
}

impl Storage {
    /// Initialize storage directory.
    pub fn init(root: &Path) -> Result<Self> {
        if !root.exists() {
            fs::create_dir_all(root)?;
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Open existing storage directory.
    pub fn open(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Get root path.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

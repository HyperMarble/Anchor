//
//  config.rs
//  Anchor
//
//  Created by hak (tharun)
//

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Top-level Anchor configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnchorConfig {
    #[serde(default)]
    pub project: ProjectConfig,
}

/// Project-level settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Root directory to scan (relative to .anchor/).
    #[serde(default = "default_root")]
    pub root: String,
    /// Languages to parse.
    #[serde(default = "default_languages")]
    pub languages: Vec<String>,
}

fn default_root() -> String {
    ".".to_string()
}

fn default_languages() -> Vec<String> {
    vec![
        "rust".to_string(),
        "python".to_string(),
        "typescript".to_string(),
        "javascript".to_string(),
    ]
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            root: default_root(),
            languages: default_languages(),
        }
    }
}

impl AnchorConfig {
    /// Load config from a TOML file, falling back to defaults.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Resolve the project root relative to the config file's parent directory.
    pub fn resolve_root(&self, anchor_dir: &Path) -> PathBuf {
        let parent = anchor_dir.parent().unwrap_or(anchor_dir);
        parent.join(&self.project.root)
    }
}

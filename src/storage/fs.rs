//! File system operations for Anchor storage.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::blueprint::Blueprint;
use crate::error::{AnchorError, Result};

/// Storage layer handling all file system operations.
pub struct Storage {
    /// Root directory (.anchor/)
    root: PathBuf,
    /// Blueprints directory (.anchor/blueprints/)
    blueprints_dir: PathBuf,
    /// Index file path (.anchor/index.json)
    index_path: PathBuf,
}

impl Storage {
    /// Initialize a new Anchor storage directory.
    ///
    /// Creates the directory structure:
    /// ```text
    /// .anchor/
    /// ├── blueprints/
    /// ├── index.json
    /// └── config.toml
    /// ```
    pub fn init(root: &Path) -> Result<Self> {
        // Create root directory
        if !root.exists() {
            fs::create_dir_all(root)?;
        }

        let blueprints_dir = root.join("blueprints");
        let index_path = root.join("index.json");
        let config_path = root.join("config.toml");

        // Create blueprints directory
        if !blueprints_dir.exists() {
            fs::create_dir(&blueprints_dir)?;
        }

        // Create empty index if it doesn't exist
        if !index_path.exists() {
            let empty_index = serde_json::json!({
                "version": "0.1.0",
                "blueprints": {}
            });
            let mut file = File::create(&index_path)?;
            file.write_all(serde_json::to_string_pretty(&empty_index)?.as_bytes())?;
        }

        // Create default config if it doesn't exist
        if !config_path.exists() {
            let default_config = r#"# Anchor Configuration

[storage]
# Directory for blueprints (relative to .anchor/)
blueprints_dir = "blueprints"

[memory]
# Enable auto-decay (not implemented yet)
auto_decay = false
"#;
            let mut file = File::create(&config_path)?;
            file.write_all(default_config.as_bytes())?;
        }

        Ok(Self {
            root: root.to_path_buf(),
            blueprints_dir,
            index_path,
        })
    }

    /// Open an existing Anchor storage directory.
    pub fn open(root: &Path) -> Result<Self> {
        if !root.exists() {
            return Err(AnchorError::NotFound(root.to_path_buf()));
        }

        let blueprints_dir = root.join("blueprints");
        let index_path = root.join("index.json");

        if !blueprints_dir.exists() {
            return Err(AnchorError::InvalidStructure(
                "Missing blueprints directory".into(),
            ));
        }

        if !index_path.exists() {
            return Err(AnchorError::InvalidStructure(
                "Missing index.json".into(),
            ));
        }

        Ok(Self {
            root: root.to_path_buf(),
            blueprints_dir,
            index_path,
        })
    }

    /// Write a blueprint to storage.
    pub fn write_blueprint(&self, blueprint: &Blueprint) -> Result<()> {
        // Validate ID
        Self::validate_id(blueprint.id())?;

        // Create the file path
        let file_path = self.blueprint_path(blueprint.id());

        // Serialize to markdown with frontmatter
        let content = blueprint.to_markdown();

        // Write atomically (write to temp, then rename)
        let temp_path = file_path.with_extension("md.tmp");
        let mut file = File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;

        fs::rename(&temp_path, &file_path)?;

        // Update index
        self.update_index(blueprint)?;

        Ok(())
    }

    /// Read a blueprint from storage.
    pub fn read_blueprint(&self, id: &str) -> Result<Blueprint> {
        let file_path = self.blueprint_path(id);

        if !file_path.exists() {
            return Err(AnchorError::BlueprintNotFound(id.to_string()));
        }

        let mut file = File::open(&file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        Blueprint::from_markdown(&content)
    }

    /// Delete a blueprint from storage.
    pub fn delete_blueprint(&self, id: &str) -> Result<()> {
        let file_path = self.blueprint_path(id);

        if !file_path.exists() {
            return Err(AnchorError::BlueprintNotFound(id.to_string()));
        }

        fs::remove_file(&file_path)?;
        self.remove_from_index(id)?;

        Ok(())
    }

    /// List all blueprint IDs.
    pub fn list_blueprints(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();

        for entry in fs::read_dir(&self.blueprints_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
                if let Some(stem) = path.file_stem() {
                    ids.push(stem.to_string_lossy().to_string());
                }
            }
        }

        ids.sort();
        Ok(ids)
    }

    /// Check if a blueprint exists.
    pub fn blueprint_exists(&self, id: &str) -> bool {
        self.blueprint_path(id).exists()
    }

    /// Get the file path for a blueprint.
    fn blueprint_path(&self, id: &str) -> PathBuf {
        self.blueprints_dir.join(format!("{}.md", id))
    }

    /// Validate a blueprint ID.
    fn validate_id(id: &str) -> Result<()> {
        if id.is_empty() {
            return Err(AnchorError::InvalidBlueprintId(
                "ID cannot be empty".to_string(),
            ));
        }

        // Allow alphanumeric, underscores, and hyphens
        let valid = id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-');

        if !valid {
            return Err(AnchorError::InvalidBlueprintId(id.to_string()));
        }

        Ok(())
    }

    /// Update the index with a blueprint's metadata.
    fn update_index(&self, blueprint: &Blueprint) -> Result<()> {
        let mut index = self.read_index()?;

        if let Some(blueprints) = index.get_mut("blueprints").and_then(|b| b.as_object_mut()) {
            blueprints.insert(
                blueprint.id().to_string(),
                serde_json::json!({
                    "updated": blueprint.meta().updated.to_rfc3339(),
                    "type": blueprint.meta().blueprint_type,
                }),
            );
        }

        let mut file = File::create(&self.index_path)?;
        file.write_all(serde_json::to_string_pretty(&index)?.as_bytes())?;

        Ok(())
    }

    /// Remove a blueprint from the index.
    fn remove_from_index(&self, id: &str) -> Result<()> {
        let mut index = self.read_index()?;

        if let Some(blueprints) = index.get_mut("blueprints").and_then(|b| b.as_object_mut()) {
            blueprints.remove(id);
        }

        let mut file = File::create(&self.index_path)?;
        file.write_all(serde_json::to_string_pretty(&index)?.as_bytes())?;

        Ok(())
    }

    /// Read the index file.
    fn read_index(&self) -> Result<serde_json::Value> {
        let mut file = File::open(&self.index_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Ok(serde_json::from_str(&content)?)
    }
}

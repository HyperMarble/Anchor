//! # Anchor SDK
//!
//! Deterministic structural memory layer for AI applications.
//!
//! Anchor provides local-first, relationship-based memory storage that AI applications
//! can use to remember, learn, and reason across sessions.
//!
//! ## Key Features
//!
//! - **Local-first**: All data stored as markdown files on the user's device
//! - **Relationship-based**: Explicit links between memories, not vector similarity
//! - **Human-readable**: Blueprints are markdown files you can read and edit
//! - **Deterministic**: Same query always returns the same result
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use anchor::{Anchor, Blueprint};
//!
//! // Initialize Anchor in a directory
//! let anchor = Anchor::init(".anchor").unwrap();
//!
//! // Create a new blueprint (memory)
//! let blueprint = anchor.create_blueprint(
//!     "my_project",
//!     "This is my project memory"
//! ).unwrap();
//!
//! // Read it back
//! let loaded = anchor.get_blueprint("my_project").unwrap();
//! ```

pub mod error;
pub mod storage;
pub mod blueprint;

// Re-exports for convenience
pub use error::{AnchorError, Result};
pub use storage::Storage;
pub use blueprint::{Blueprint, BlueprintMeta};

use std::path::PathBuf;

/// The main Anchor instance.
///
/// This is the primary interface for interacting with Anchor's memory system.
pub struct Anchor {
    /// Root directory for Anchor storage (.anchor/)
    root: PathBuf,
    /// Storage layer for file operations
    storage: Storage,
}

impl Anchor {
    /// Initialize Anchor in the specified directory.
    ///
    /// Creates the `.anchor/` directory structure if it doesn't exist:
    /// ```text
    /// .anchor/
    /// ├── blueprints/     # Individual memory files
    /// ├── index.json      # Master index
    /// └── config.toml     # Configuration
    /// ```
    pub fn init<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let root = path.into();
        let storage = Storage::init(&root)?;

        Ok(Self { root, storage })
    }

    /// Open an existing Anchor directory.
    ///
    /// Returns an error if the directory doesn't exist or isn't a valid Anchor store.
    pub fn open<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let root = path.into();
        let storage = Storage::open(&root)?;

        Ok(Self { root, storage })
    }

    /// Create a new blueprint (memory) in the store.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this blueprint (e.g., "my_project", "patient_123")
    /// * `content` - The markdown content of the blueprint
    ///
    /// # Returns
    /// The created Blueprint with generated metadata
    pub fn create_blueprint(&self, id: &str, content: &str) -> Result<Blueprint> {
        let blueprint = Blueprint::new(id, content);
        self.storage.write_blueprint(&blueprint)?;
        Ok(blueprint)
    }

    /// Get a blueprint by its ID.
    pub fn get_blueprint(&self, id: &str) -> Result<Blueprint> {
        self.storage.read_blueprint(id)
    }

    /// Update an existing blueprint's content.
    pub fn update_blueprint(&self, id: &str, content: &str) -> Result<Blueprint> {
        let mut blueprint = self.get_blueprint(id)?;
        blueprint.update_content(content);
        self.storage.write_blueprint(&blueprint)?;
        Ok(blueprint)
    }

    /// Delete a blueprint from the store.
    pub fn delete_blueprint(&self, id: &str) -> Result<()> {
        self.storage.delete_blueprint(id)
    }

    /// List all blueprint IDs in the store.
    pub fn list_blueprints(&self) -> Result<Vec<String>> {
        self.storage.list_blueprints()
    }

    /// Check if a blueprint exists.
    pub fn has_blueprint(&self, id: &str) -> bool {
        self.storage.blueprint_exists(id)
    }

    /// Get the root path of this Anchor instance.
    pub fn root(&self) -> &PathBuf {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_init_and_create() {
        let dir = tempdir().unwrap();
        let anchor_path = dir.path().join(".anchor");

        let anchor = Anchor::init(&anchor_path).unwrap();

        // Create a blueprint
        let bp = anchor.create_blueprint("test_project", "# Test\n\nThis is a test.").unwrap();
        assert_eq!(bp.id(), "test_project");

        // Read it back
        let loaded = anchor.get_blueprint("test_project").unwrap();
        assert_eq!(loaded.content(), "# Test\n\nThis is a test.");

        // List blueprints
        let list = anchor.list_blueprints().unwrap();
        assert_eq!(list, vec!["test_project"]);
    }
}

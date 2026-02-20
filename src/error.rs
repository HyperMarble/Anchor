//
//  error.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::path::PathBuf;
use thiserror::Error;

/// Result type for Anchor operations.
pub type Result<T> = std::result::Result<T, AnchorError>;

/// Errors that can occur when using Anchor.
#[derive(Error, Debug)]
pub enum AnchorError {
    /// The Anchor directory doesn't exist.
    #[error("Anchor directory not found: {0}")]
    NotFound(PathBuf),

    /// The Anchor directory already exists (when trying to init).
    #[error("Anchor directory already exists: {0}")]
    AlreadyExists(PathBuf),

    /// A blueprint with this ID was not found.
    #[error("Blueprint not found: {0}")]
    BlueprintNotFound(String),

    /// A blueprint with this ID already exists.
    #[error("Blueprint already exists: {0}")]
    BlueprintAlreadyExists(String),

    /// Invalid blueprint ID (contains invalid characters).
    #[error("Invalid blueprint ID: {0}. IDs must be alphanumeric with underscores/hyphens.")]
    InvalidBlueprintId(String),

    /// Failed to parse blueprint frontmatter.
    #[error("Failed to parse blueprint frontmatter: {0}")]
    ParseError(String),

    /// Failed to serialize data.
    #[error("Serialization error: {0}")]
    SerializeError(String),

    /// File system error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON parsing error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Invalid Anchor directory structure.
    #[error("Invalid Anchor directory structure: {0}")]
    InvalidStructure(String),

    /// File has an unsupported language (no tree-sitter grammar available).
    #[error("Unsupported language for file: {0}")]
    UnsupportedLanguage(PathBuf),

    /// tree-sitter parser failed to initialize for a language.
    #[error("Parser init failed for {0}: {1}")]
    ParserInitError(PathBuf, String),

    /// tree-sitter returned None from parse (e.g., timeout or cancellation).
    #[error("tree-sitter parse failed for: {0}")]
    TreeSitterParseFailed(PathBuf),
}

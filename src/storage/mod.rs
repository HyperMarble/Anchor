//! Storage layer for Anchor.
//!
//! Handles all file system operations:
//! - Creating/managing the `.anchor/` directory structure
//! - Reading/writing blueprint files
//! - Managing the index

mod fs;

pub use fs::Storage;

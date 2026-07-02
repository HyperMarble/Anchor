//
//  lib.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod cache;
pub mod cli;
pub mod error;
pub mod events;
pub mod lock;
pub mod parser;
pub mod query;
pub mod storage;
pub mod write;

// Re-exports for convenience
pub use error::{AnchorError, Result};
pub use parser::SupportedLanguage;

// Write operations
pub use write::{create_file, insert_after, replace_all, WriteError, WriteResult};

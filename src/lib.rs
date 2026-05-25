//
//  lib.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod cache;
pub mod cli;
pub mod config;
pub mod error;
pub mod lock;
pub mod parser;
pub mod query;
pub mod regex;
pub mod storage;
pub mod updater;
pub mod write;

// Re-exports for convenience
pub use error::{AnchorError, Result};
pub use parser::SupportedLanguage;

// Write operations
pub use write::{
    create_file, insert_after, insert_before, replace_all, replace_first, WriteError, WriteResult,
};

// Regex engine (Brzozowski derivatives - ReDoS-safe)
pub use regex::{parse as parse_regex, Matcher as RegexMatcher, Regex};

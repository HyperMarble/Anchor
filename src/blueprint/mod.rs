//! Blueprint - the core memory unit in Anchor.
//!
//! A Blueprint is a single unit of memory stored as a markdown file.
//! Each domain/project gets ONE blueprint file containing all its structured data.

mod types;

pub use types::{Blueprint, BlueprintMeta};

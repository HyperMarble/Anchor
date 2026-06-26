//
//  write.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

use crate::cli::protect;
use crate::events;
use crate::lock::lockd;
use crate::parser::language::is_source_path;
use crate::storage::{content_hash, AnchorStore};
use crate::write::{
    batch_replace_all, create_file, insert_after, replace_all, replace_range, BatchWriteResult,
};

include!("write_parts/locks.rs");
include!("write_parts/freshness.rs");
include!("write_parts/create.rs");
include!("write_parts/insert.rs");
include!("write_parts/replace_symbol.rs");
include!("write_parts/replace.rs");
include!("write_parts/glob.rs");

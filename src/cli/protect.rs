use anyhow::{bail, Result};
use ignore::Walk;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::events;
use crate::parser::language::is_source_path;
use crate::storage::AnchorStore;

const PROTECTION_SCHEMA: &str = "anchor.protection.v1";
const PROTECTION_FILE: &str = "protection.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProtectionState {
    schema: String,
    entries: Vec<ProtectionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProtectionEntry {
    path: String,
    kind: ProtectionKind,
    mode: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProtectionKind {
    File,
    Dir,
}

include!("protect_parts/public.rs");
include!("protect_parts/actions.rs");
include!("protect_parts/state_guard.rs");
include!("protect_parts/helpers.rs");

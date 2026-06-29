use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AnchorError, Result};
use crate::parser::language::is_source_path;

pub const ANCHOR_DIR: &str = ".anchor";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Parse,
    Slice,
    Patch,
}

impl ObjectKind {
    fn dir_name(self) -> &'static str {
        match self {
            Self::Parse => "parses",
            Self::Slice => "slices",
            Self::Patch => "patches",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnchorStore {
    repo_root: PathBuf,
    anchor_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathEntry {
    pub path: String,
    pub source_hash: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathIndex {
    pub files: Vec<PathEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolEntry {
    pub path: String,
    pub source_hash: String,
    pub name: String,
    pub kind: String,
    pub line_start: usize,
    pub line_end: usize,
    pub slice_hash: String,
    /// Pre-computed sub-tokens from camelCase/snake_case splitting for intent-based search.
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolIndex {
    pub symbols: Vec<SymbolEntry>,
}

/// Call index: maps each symbol name to the names of symbols it calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CallIndex {
    pub calls: HashMap<String, Vec<String>>,
}

impl CallIndex {
    /// All symbols that call `name`.
    pub fn callers_of<'a>(&'a self, name: &str) -> Vec<&'a str> {
        self.calls
            .iter()
            .filter(|(_, callees)| callees.iter().any(|c| c == name))
            .map(|(caller, _)| caller.as_str())
            .collect()
    }

    /// All symbols that `name` calls.
    pub fn callees_of(&self, name: &str) -> Vec<&str> {
        self.calls
            .get(name)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryIndex {
    pub schema: String,
    pub commits_scanned: usize,
    pub cochanges: Vec<CoChangeEntry>,
    #[serde(default)]
    pub adjacency: BTreeMap<String, Vec<HistoryNeighbor>>,
    pub paths: Vec<PathHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoChangeEntry {
    pub path: String,
    pub related_path: String,
    pub commits: usize,
    #[serde(default)]
    pub score: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryNeighbor {
    pub related_path: String,
    pub commits: usize,
    pub score: usize,
    pub is_test: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathHistoryEntry {
    pub path: String,
    pub commits: usize,
    #[serde(default)]
    pub score: usize,
    pub is_test: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMemory {
    pub schema: String,
    pub source_hash: String,
    pub facts: Vec<ProductMemoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMemoryEntry {
    pub source: String,
    pub fact: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    pub path: String,
    pub source_hash: String,
    pub symbol: String,
    pub line_start: usize,
    pub line_end: usize,
    pub slice_hash: String,
    pub prefix_hash: String,
    pub suffix_hash: String,
    pub text: String,
}

include!("anchor_parts/store_core.rs");
include!("anchor_parts/index_io.rs");
include!("anchor_parts/symbol_update.rs");
include!("anchor_parts/search_projection.rs");
fn scored_symbol_rank(symbol: &SymbolEntry, query: &str, bm25_score: f32) -> f32 {
    let query_lower = query.to_lowercase();
    let name_lower = symbol.name.to_lowercase();
    let path = Path::new(&symbol.path);
    let source_bonus = if is_source_path(path) { 80.0 } else { -60.0 };
    let kind_bonus = match symbol.kind.as_str() {
        "Class" | "Function" | "Method" | "Struct" | "Enum" | "Interface" | "Trait" => 25.0,
        "Module" if is_source_path(path) => 5.0,
        "Module" => -30.0,
        _ => 0.0,
    };
    let name_bonus = if name_lower == query_lower {
        500.0
    } else if name_lower.starts_with(&query_lower) {
        160.0
    } else if name_lower.contains(&query_lower) {
        80.0
    } else {
        0.0
    };
    let test_penalty = if symbol.path.contains("/tests/") || symbol.path.starts_with("tests/") {
        -15.0
    } else {
        0.0
    };

    bm25_score + source_bonus + kind_bonus + name_bonus + test_penalty
}

fn score_symbol_match(symbol: &SymbolEntry, query_lower: &str) -> usize {
    let name_lower = symbol.name.to_lowercase();
    if name_lower == query_lower {
        return 0;
    }
    if name_lower.starts_with(query_lower) {
        return 1;
    }
    if name_lower.contains(query_lower) {
        return 2;
    }
    3
}

pub fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(());
    }

    Err(AnchorError::InvalidStructure(format!(
        "invalid object hash: {hash}"
    )))
}

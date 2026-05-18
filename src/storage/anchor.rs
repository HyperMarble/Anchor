use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AnchorError, Result};

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

/// Call graph index: maps each symbol name to the names of symbols it calls.
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

impl AnchorStore {
    pub fn init(repo_root: &Path) -> Result<Self> {
        let repo_root = repo_root.to_path_buf();
        let anchor_root = repo_root.join(ANCHOR_DIR);

        fs::create_dir_all(anchor_root.join("objects").join("parses"))?;
        fs::create_dir_all(anchor_root.join("objects").join("slices"))?;
        fs::create_dir_all(anchor_root.join("objects").join("patches"))?;
        fs::create_dir_all(anchor_root.join("index"))?;
        fs::create_dir_all(anchor_root.join("locks").join("ranges"))?;
        fs::create_dir_all(anchor_root.join("projections"))?;
        fs::create_dir_all(anchor_root.join("writes"))?;

        Ok(Self {
            repo_root,
            anchor_root,
        })
    }

    pub fn open(repo_root: &Path) -> Result<Self> {
        let anchor_root = repo_root.join(ANCHOR_DIR);
        if !anchor_root.is_dir() {
            return Err(AnchorError::NotFound(anchor_root));
        }

        Ok(Self {
            repo_root: repo_root.to_path_buf(),
            anchor_root,
        })
    }

    pub fn discover(start: &Path) -> Result<Self> {
        let mut current = if start.is_file() {
            start
                .parent()
                .ok_or_else(|| AnchorError::NotFound(start.to_path_buf()))?
                .to_path_buf()
        } else {
            start.to_path_buf()
        };

        loop {
            if current.join(ANCHOR_DIR).is_dir() {
                return Self::open(&current);
            }

            if !current.pop() {
                return Err(AnchorError::NotFound(start.join(ANCHOR_DIR)));
            }
        }
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn anchor_root(&self) -> &Path {
        &self.anchor_root
    }

    pub fn object_path(&self, kind: ObjectKind, hash: &str) -> Result<PathBuf> {
        validate_hash(hash)?;
        Ok(self
            .anchor_root
            .join("objects")
            .join(kind.dir_name())
            .join(&hash[..2])
            .join(format!("{hash}.json")))
    }

    pub fn write_object(&self, kind: ObjectKind, hash: &str, bytes: &[u8]) -> Result<bool> {
        let path = self.object_path(kind, hash)?;
        if path.exists() {
            return Ok(false);
        }

        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!("object path has no parent: {}", path.display()))
        })?)?;
        fs::write(path, bytes)?;
        Ok(true)
    }

    pub fn read_object(&self, kind: ObjectKind, hash: &str) -> Result<Vec<u8>> {
        let path = self.object_path(kind, hash)?;
        Ok(fs::read(path)?)
    }

    pub fn path_index_path(&self) -> PathBuf {
        self.anchor_root.join("index").join("paths.json")
    }

    pub fn symbol_index_path(&self) -> PathBuf {
        self.anchor_root.join("index").join("symbols.json")
    }

    pub fn load_path_index(&self) -> Result<PathIndex> {
        let path = self.path_index_path();
        if !path.exists() {
            return Ok(PathIndex::default());
        }

        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_path_index(&self, index: &PathIndex) -> Result<()> {
        let path = self.path_index_path();
        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!("path index has no parent: {}", path.display()))
        })?)?;
        fs::write(path, serde_json::to_vec_pretty(index)?)?;
        Ok(())
    }

    pub fn upsert_path(&self, source_path: &Path) -> Result<(PathEntry, bool)> {
        let bytes = fs::read(source_path)?;
        let entry = PathEntry {
            path: self.repo_relative_path(source_path)?,
            source_hash: content_hash(&bytes),
            bytes: bytes.len() as u64,
        };

        let mut index = self.load_path_index()?;
        let mut changed = true;

        if let Some(existing) = index.files.iter_mut().find(|item| item.path == entry.path) {
            if existing == &entry {
                changed = false;
            } else {
                *existing = entry.clone();
            }
        } else {
            index.files.push(entry.clone());
        }

        if changed {
            index.files.sort_by(|a, b| a.path.cmp(&b.path));
            self.save_path_index(&index)?;
        }

        Ok((entry, changed))
    }

    pub fn load_symbol_index(&self) -> Result<SymbolIndex> {
        let path = self.symbol_index_path();
        if !path.exists() {
            return Ok(SymbolIndex::default());
        }

        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_symbol_index(&self, index: &SymbolIndex) -> Result<()> {
        let path = self.symbol_index_path();
        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!("symbol index has no parent: {}", path.display()))
        })?)?;
        fs::write(path, serde_json::to_vec_pretty(index)?)?;
        Ok(())
    }

    pub fn call_index_path(&self) -> PathBuf {
        self.anchor_root.join("index").join("calls.json")
    }

    pub fn save_call_index(&self, index: &CallIndex) -> Result<()> {
        let path = self.call_index_path();
        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!("call index has no parent: {}", path.display()))
        })?)?;
        fs::write(path, serde_json::to_vec(index)?)?;
        Ok(())
    }

    pub fn load_call_index(&self) -> CallIndex {
        let Ok(bytes) = fs::read(self.call_index_path()) else {
            return CallIndex::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    /// Symbols in a file that overlap the given line range. Used for write impact analysis.
    pub fn symbols_in_range<'a>(
        &self,
        index: &'a SymbolIndex,
        path: &str,
        start: usize,
        end: usize,
    ) -> Vec<&'a SymbolEntry> {
        index
            .symbols
            .iter()
            .filter(|s| s.path == path && s.line_start <= end && s.line_end >= start)
            .collect()
    }

    pub fn upsert_symbols_for_path(
        &self,
        source_path: &Path,
    ) -> Result<(PathEntry, Vec<SymbolEntry>, bool)> {
        let source = fs::read_to_string(source_path)?;
        let extraction = crate::parser::extract_file(source_path, &source)?;
        let (path_entry, path_changed) = self.upsert_path(source_path)?;

        let mut symbols: Vec<SymbolEntry> = extraction
            .symbols
            .iter()
            .map(|symbol| SymbolEntry {
                path: path_entry.path.clone(),
                source_hash: path_entry.source_hash.clone(),
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                line_start: symbol.line_start,
                line_end: symbol.line_end,
                slice_hash: content_hash(symbol.code_snippet.as_bytes()),
                features: symbol.features.clone(),
            })
            .collect();
        symbols.sort_by(|a, b| {
            a.line_start
                .cmp(&b.line_start)
                .then_with(|| a.name.cmp(&b.name))
        });

        let mut index = self.load_symbol_index()?;
        let existing: Vec<SymbolEntry> = index
            .symbols
            .iter()
            .filter(|symbol| symbol.path == path_entry.path)
            .cloned()
            .collect();
        let changed = path_changed || existing != symbols;

        if changed {
            index
                .symbols
                .retain(|symbol| symbol.path != path_entry.path);
            index.symbols.extend(symbols.clone());
            index.symbols.sort_by(|a, b| {
                a.path
                    .cmp(&b.path)
                    .then_with(|| a.line_start.cmp(&b.line_start))
                    .then_with(|| a.name.cmp(&b.name))
            });
            self.save_symbol_index(&index)?;
        }

        Ok((path_entry, symbols, changed))
    }

    pub fn search_symbols(&self, query: &str, limit: usize) -> Result<Vec<SymbolEntry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let query_lower = query.to_lowercase();
        let mut matches: Vec<SymbolEntry> = self
            .load_symbol_index()?
            .symbols
            .into_iter()
            .filter(|symbol| {
                symbol.name.to_lowercase().contains(&query_lower)
                    || symbol.path.to_lowercase().contains(&query_lower)
            })
            .collect();

        matches.sort_by(|a, b| {
            score_symbol_match(a, &query_lower)
                .cmp(&score_symbol_match(b, &query_lower))
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.line_start.cmp(&b.line_start))
                .then_with(|| a.name.cmp(&b.name))
        });
        matches.truncate(limit);

        Ok(matches)
    }

    pub fn create_projection(&self, symbol: &SymbolEntry) -> Result<Projection> {
        let source_path = self.repo_root.join(&symbol.path);
        let source = fs::read_to_string(&source_path)?;
        let current_hash = content_hash(source.as_bytes());
        if current_hash != symbol.source_hash {
            return Err(AnchorError::InvalidStructure(format!(
                "stale symbol index for {}: expected {}, got {}",
                symbol.path, symbol.source_hash, current_hash
            )));
        }

        let lines: Vec<&str> = source.lines().collect();
        if symbol.line_start < 1
            || symbol.line_end < symbol.line_start
            || symbol.line_end > lines.len()
        {
            return Err(AnchorError::InvalidStructure(format!(
                "invalid projection range {}:{}-{}",
                symbol.path, symbol.line_start, symbol.line_end
            )));
        }

        let slice = lines[symbol.line_start - 1..symbol.line_end].join("\n");
        let prefix = lines[..symbol.line_start - 1].join("\n");
        let suffix = lines[symbol.line_end..].join("\n");

        Ok(Projection {
            path: symbol.path.clone(),
            source_hash: symbol.source_hash.clone(),
            symbol: symbol.name.clone(),
            line_start: symbol.line_start,
            line_end: symbol.line_end,
            slice_hash: content_hash(slice.as_bytes()),
            prefix_hash: content_hash(prefix.as_bytes()),
            suffix_hash: content_hash(suffix.as_bytes()),
            text: slice,
        })
    }

    /// Search symbols by text match. Uses pre-computed features for identifier-aware matching.
    pub fn search_symbols_hybrid(&self, query: &str, limit: usize) -> Result<Vec<SymbolEntry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let index = self.load_symbol_index()?;
        let query_lower = query.to_lowercase();
        let query_tokens: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<(SymbolEntry, f32)> = index
            .symbols
            .into_iter()
            .filter_map(|sym| {
                let name_lower = sym.name.to_lowercase();
                let path_lower = sym.path.to_lowercase();

                let hit = if query_tokens.len() > 1 {
                    // Multi-word: check name, path, and pre-computed features
                    let hits = query_tokens.iter().filter(|t| {
                        name_lower.contains(*t)
                            || path_lower.contains(*t)
                            || sym.features.iter().any(|f| f == *t)
                    }).count();
                    hits * 2 >= query_tokens.len()
                } else {
                    name_lower.contains(&query_lower)
                        || path_lower.contains(&query_lower)
                        || sym.features.iter().any(|f| f == &query_lower)
                };

                if hit {
                    let score = score_symbol_match_f32(&sym, &query_lower);
                    Some((sym, score))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(sym, _)| sym).collect())
    }

    fn repo_relative_path(&self, path: &Path) -> Result<String> {
        let relative = path.strip_prefix(&self.repo_root).map_err(|_| {
            AnchorError::InvalidStructure(format!(
                "path is outside Anchor repo root: {}",
                path.display()
            ))
        })?;

        Ok(relative.to_string_lossy().replace('\\', "/"))
    }
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

fn score_symbol_match_f32(symbol: &SymbolEntry, query_lower: &str) -> f32 {
    let name_lower = symbol.name.to_lowercase();
    let path_lower = symbol.path.to_lowercase();
    let tokens: Vec<&str> = query_lower.split_whitespace().collect();

    if tokens.len() > 1 {
        // Multi-word: count hits across name, features, and path
        let hits = tokens.iter().filter(|t| {
            name_lower.contains(*t)
                || path_lower.contains(*t)
                || symbol.features.iter().any(|f| f == *t)
        }).count();
        // Name-or-feature hits weighted higher than path hits
        let name_hits = tokens.iter().filter(|t| {
            name_lower.contains(*t) || symbol.features.iter().any(|f| f == *t)
        }).count();
        return (hits as f32 / tokens.len() as f32) * 0.7
            + (name_hits as f32 / tokens.len() as f32) * 0.3;
    }

    if name_lower == query_lower { return 1.0; }
    if name_lower.starts_with(query_lower) { return 0.9; }
    // Feature exact match: "lock" hits LockManager's ["lock","manager"] — beats substring noise
    if symbol.features.iter().any(|f| f == query_lower) { return 0.85; }
    if name_lower.contains(query_lower) { return 0.7; }
    if path_lower.contains(query_lower) { return 0.5; }
    0.3
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


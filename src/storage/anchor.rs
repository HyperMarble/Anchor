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

            // A `.git` directory marks the project boundary. Walking past it
            // can silently attach to an unrelated store higher up the disk
            // (e.g. a stray `.anchor` at a mount root), which mixes state
            // across every repo below it.
            if current.join(".git").exists() {
                return Err(AnchorError::NotFound(start.join(ANCHOR_DIR)));
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

    pub fn history_index_path(&self) -> PathBuf {
        self.anchor_root.join("index").join("history.json")
    }

    pub fn save_history_index(&self, index: &HistoryIndex) -> Result<()> {
        let path = self.history_index_path();
        fs::create_dir_all(path.parent().ok_or_else(|| {
            AnchorError::InvalidStructure(format!(
                "history index has no parent: {}",
                path.display()
            ))
        })?)?;
        fs::write(path, serde_json::to_vec_pretty(index)?)?;
        Ok(())
    }

    pub fn load_history_index(&self) -> HistoryIndex {
        let Ok(bytes) = fs::read(self.history_index_path()) else {
            return HistoryIndex::default();
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

    /// Absolute line numbers (1-indexed) of every call site inside a symbol's body.
    /// Re-parses the file to recover line info (the saved CallIndex drops it).
    /// Used by `slice_code` to keep only call-relevant lines in projections.
    pub fn call_lines_for_symbol(&self, sym: &SymbolEntry) -> Vec<usize> {
        let source_path = self.repo_root.join(&sym.path);
        let source = match fs::read_to_string(&source_path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let extraction = match crate::parser::extract_file(&source_path, &source) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        extraction
            .calls
            .into_iter()
            .filter(|c| c.line >= sym.line_start && c.line <= sym.line_end)
            .map(|c| c.line)
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

    /// Search symbols using BM25 ranking with camelCase/snake_case tokenization.
    /// Name-token matches get a 3x boost over path/parent/kind context matches.
    /// Falls back to substring search for queries that tokenize to nothing (e.g. "id").
    pub fn search_symbols_hybrid(&self, query: &str, limit: usize) -> Result<Vec<SymbolEntry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let query_tokens = crate::storage::bm25::tokenize(query);
        if query_tokens.is_empty() {
            return self.search_symbols(query, limit);
        }

        let index = self.load_symbol_index()?;
        let bm25 = crate::storage::bm25::Bm25Index::build(&index.symbols);

        let mut scored: Vec<(SymbolEntry, f32)> = index
            .symbols
            .into_iter()
            .filter_map(|sym| {
                let score = bm25.score(&sym, &query_tokens);
                if score > 0.0 {
                    Some((sym, score))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            scored_symbol_rank(&b.0, query, b.1)
                .partial_cmp(&scored_symbol_rank(&a.0, query, a.1))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.path.cmp(&b.0.path))
                .then_with(|| a.0.line_start.cmp(&b.0.line_start))
                .then_with(|| a.0.name.cmp(&b.0.name))
        });
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

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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolIndex {
    pub symbols: Vec<SymbolEntry>,
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn content_hash_is_stable_sha256_hex() {
        let hash = content_hash(b"anchor");

        assert_eq!(hash.len(), 64);
        assert_eq!(hash, content_hash(b"anchor"));
        assert_ne!(hash, content_hash(b"anchor changed"));
    }

    #[test]
    fn init_creates_git_style_anchor_layout() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();

        assert_eq!(store.repo_root(), dir.path());
        assert!(store.anchor_root().join("objects/parses").is_dir());
        assert!(store.anchor_root().join("objects/slices").is_dir());
        assert!(store.anchor_root().join("objects/patches").is_dir());
        assert!(store.anchor_root().join("index").is_dir());
        assert!(store.anchor_root().join("locks/ranges").is_dir());
        assert!(store.anchor_root().join("projections").is_dir());
        assert!(store.anchor_root().join("writes").is_dir());
    }

    #[test]
    fn discover_finds_parent_anchor_dir() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("src/deep");
        fs::create_dir_all(&nested).unwrap();
        AnchorStore::init(dir.path()).unwrap();

        let store = AnchorStore::discover(&nested).unwrap();

        assert_eq!(store.repo_root(), dir.path());
    }

    #[test]
    fn object_path_uses_hash_prefix_directory() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let hash = content_hash(b"source");

        let path = store.object_path(ObjectKind::Parse, &hash).unwrap();

        assert_eq!(
            path,
            store
                .anchor_root()
                .join("objects/parses")
                .join(&hash[..2])
                .join(format!("{hash}.json"))
        );
    }

    #[test]
    fn objects_are_content_addressed_and_not_rewritten() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let bytes = br#"{"path":"src/lib.rs"}"#;
        let hash = content_hash(bytes);

        assert!(store.write_object(ObjectKind::Parse, &hash, bytes).unwrap());
        assert!(!store.write_object(ObjectKind::Parse, &hash, bytes).unwrap());
        assert_eq!(store.read_object(ObjectKind::Parse, &hash).unwrap(), bytes);
    }

    #[test]
    fn missing_path_index_loads_as_empty() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();

        let index = store.load_path_index().unwrap();

        assert!(index.files.is_empty());
    }

    #[test]
    fn upsert_path_writes_repo_relative_hash_entry() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let source = dir.path().join("src/lib.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "pub fn run() {}\n").unwrap();

        let (entry, changed) = store.upsert_path(&source).unwrap();

        assert!(changed);
        assert_eq!(entry.path, "src/lib.rs");
        assert_eq!(entry.bytes, 16);
        assert_eq!(entry.source_hash, content_hash(b"pub fn run() {}\n"));

        let index = store.load_path_index().unwrap();
        assert_eq!(index.files, vec![entry]);
    }

    #[test]
    fn unchanged_path_does_not_rewrite_index_entry() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let source = dir.path().join("src/lib.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "pub fn run() {}\n").unwrap();

        let (first, first_changed) = store.upsert_path(&source).unwrap();
        let (second, second_changed) = store.upsert_path(&source).unwrap();

        assert!(first_changed);
        assert!(!second_changed);
        assert_eq!(first, second);
        assert_eq!(store.load_path_index().unwrap().files.len(), 1);
    }

    #[test]
    fn changed_path_refreshes_hash_in_place() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let source = dir.path().join("src/lib.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "pub fn run() {}\n").unwrap();
        let (first, _) = store.upsert_path(&source).unwrap();

        fs::write(&source, "pub fn run_fast() {}\n").unwrap();
        let (second, changed) = store.upsert_path(&source).unwrap();

        assert!(changed);
        assert_eq!(second.path, "src/lib.rs");
        assert_ne!(first.source_hash, second.source_hash);
        assert_eq!(store.load_path_index().unwrap().files, vec![second]);
    }

    #[test]
    fn path_index_rejects_files_outside_repo_root() {
        let dir = tempdir().unwrap();
        let other = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let outside = other.path().join("lib.rs");
        fs::write(&outside, "pub fn outside() {}\n").unwrap();

        let result = store.upsert_path(&outside);

        assert!(matches!(result, Err(AnchorError::InvalidStructure(_))));
    }

    #[test]
    fn missing_symbol_index_loads_as_empty() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();

        let index = store.load_symbol_index().unwrap();

        assert!(index.symbols.is_empty());
    }

    #[test]
    fn upsert_symbols_for_path_indexes_parser_symbols() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let source = dir.path().join("src/lib.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(
            &source,
            "pub struct Service;\n\npub fn run() {\n    helper();\n}\n\nfn helper() {}\n",
        )
        .unwrap();

        let (path_entry, symbols, changed) = store.upsert_symbols_for_path(&source).unwrap();

        assert!(changed);
        assert_eq!(path_entry.path, "src/lib.rs");
        assert_eq!(symbols.len(), 3);
        assert!(symbols.iter().any(|symbol| symbol.name == "Service"));
        assert!(symbols.iter().any(|symbol| symbol.name == "run"));
        assert!(symbols.iter().any(|symbol| symbol.name == "helper"));
        assert!(symbols
            .iter()
            .all(|symbol| symbol.source_hash == path_entry.source_hash));

        let index = store.load_symbol_index().unwrap();
        assert_eq!(index.symbols, symbols);
    }

    #[test]
    fn unchanged_symbols_do_not_rewrite_symbol_index() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let source = dir.path().join("src/lib.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "pub fn run() {}\n").unwrap();

        let (_, first, first_changed) = store.upsert_symbols_for_path(&source).unwrap();
        let (_, second, second_changed) = store.upsert_symbols_for_path(&source).unwrap();

        assert!(first_changed);
        assert!(!second_changed);
        assert_eq!(first, second);
    }

    #[test]
    fn changed_file_replaces_only_that_files_symbols() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let first = dir.path().join("src/first.rs");
        let second = dir.path().join("src/second.rs");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::write(&first, "pub fn old_name() {}\n").unwrap();
        fs::write(&second, "pub fn stable() {}\n").unwrap();
        store.upsert_symbols_for_path(&first).unwrap();
        store.upsert_symbols_for_path(&second).unwrap();

        fs::write(&first, "pub fn new_name() {}\n").unwrap();
        let (_, symbols, changed) = store.upsert_symbols_for_path(&first).unwrap();

        assert!(changed);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "new_name");

        let index = store.load_symbol_index().unwrap();
        assert!(index.symbols.iter().any(|symbol| symbol.name == "new_name"));
        assert!(index.symbols.iter().any(|symbol| symbol.name == "stable"));
        assert!(!index.symbols.iter().any(|symbol| symbol.name == "old_name"));
    }

    #[test]
    fn search_symbols_returns_compact_index_hits() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let source = dir.path().join("src/lib.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(
            &source,
            "pub fn authenticate() {}\npub fn authenticate_user() {}\npub fn logout() {}\n",
        )
        .unwrap();
        store.upsert_symbols_for_path(&source).unwrap();

        let hits = store.search_symbols("authenticate", 10).unwrap();

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "authenticate");
        assert_eq!(hits[1].name, "authenticate_user");
        assert!(hits.iter().all(|hit| hit.path == "src/lib.rs"));
    }

    #[test]
    fn search_symbols_honors_limit() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let source = dir.path().join("src/lib.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(
            &source,
            "pub fn handle_one() {}\npub fn handle_two() {}\npub fn handle_three() {}\n",
        )
        .unwrap();
        store.upsert_symbols_for_path(&source).unwrap();

        let hits = store.search_symbols("handle", 2).unwrap();

        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn search_symbols_can_match_path() {
        let dir = tempdir().unwrap();
        let store = AnchorStore::init(dir.path()).unwrap();
        let source = dir.path().join("src/auth/session.rs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "pub fn load() {}\n").unwrap();
        store.upsert_symbols_for_path(&source).unwrap();

        let hits = store.search_symbols("auth/session", 10).unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "load");
        assert_eq!(hits[0].path, "src/auth/session.rs");
    }
}

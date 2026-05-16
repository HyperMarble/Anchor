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
}

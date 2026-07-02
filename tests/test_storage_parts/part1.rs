use std::fs;
use std::path::{Path, PathBuf};

use anchor::error::AnchorError;
use anchor::storage::{content_hash, AnchorStore, ObjectKind};
use tempfile::tempdir;

#[derive(Debug)]
struct StoreProjectionBenchmark {
    files_seen: usize,
    symbols_tested: usize,
    avg_context_reduction_percent: f64,
    median_context_reduction_percent: f64,
    p90_context_reduction_percent: f64,
    min_context_reduction_percent: f64,
    max_context_reduction_percent: f64,
    avg_full_context_bytes: f64,
    avg_projection_bytes: f64,
    stale_rejections: usize,
    failures: usize,
}

fn collect_python_files(root: &Path, out: &mut Vec<PathBuf>, max_files: usize) {
    if out.len() >= max_files {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= max_files {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_python_files(&path, out, max_files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("py") {
            let Ok(meta) = entry.metadata() else { continue };
            if (5_000..=90_000).contains(&meta.len()) {
                out.push(path);
            }
        }
    }
}

fn percentile(values: &[f64], p: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn context_reduction_percent(full_bytes: usize, projection_bytes: usize) -> f64 {
    100.0 - ((projection_bytes as f64 / full_bytes as f64) * 100.0)
}

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
    assert!(store.load_path_index().unwrap().files.is_empty());
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
    assert_eq!(store.load_path_index().unwrap().files, vec![entry]);
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


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

#[test]
fn path_index_rejects_files_outside_repo_root() {
    let dir = tempdir().unwrap();
    let other = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let outside = other.path().join("lib.rs");
    fs::write(&outside, "pub fn outside() {}\n").unwrap();
    assert!(matches!(
        store.upsert_path(&outside),
        Err(AnchorError::InvalidStructure(_))
    ));
}

#[test]
fn missing_symbol_index_loads_as_empty() {
    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    assert!(store.load_symbol_index().unwrap().symbols.is_empty());
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
    assert!(symbols.iter().any(|s| s.name == "Service"));
    assert!(symbols.iter().any(|s| s.name == "run"));
    assert!(symbols.iter().any(|s| s.name == "helper"));
    assert!(symbols
        .iter()
        .all(|s| s.source_hash == path_entry.source_hash));
    assert_eq!(store.load_symbol_index().unwrap().symbols, symbols);
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
    assert!(index.symbols.iter().any(|s| s.name == "new_name"));
    assert!(index.symbols.iter().any(|s| s.name == "stable"));
    assert!(!index.symbols.iter().any(|s| s.name == "old_name"));
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
    assert!(hits.iter().all(|h| h.path == "src/lib.rs"));
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

    assert_eq!(store.search_symbols("handle", 2).unwrap().len(), 2);
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

#[test]
fn create_projection_returns_only_indexed_symbol_slice() {
    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let source = dir.path().join("src/lib.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "pub fn before() {}\n\npub fn target() {\n    before();\n}\n\npub fn after() {}\n",
    )
    .unwrap();
    store.upsert_symbols_for_path(&source).unwrap();
    let target = store.search_symbols("target", 1).unwrap().remove(0);

    let projection = store.create_projection(&target).unwrap();

    assert_eq!(projection.path, "src/lib.rs");
    assert_eq!(projection.symbol, "target");
    assert!(projection.text.contains("pub fn target()"));
    assert!(projection.text.contains("before();"));
    assert!(!projection.text.contains("pub fn before()"));
    assert!(!projection.text.contains("pub fn after()"));
    assert_eq!(
        projection.slice_hash,
        content_hash(projection.text.as_bytes())
    );
}

#[test]
fn create_projection_rejects_stale_symbol_hash() {
    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let source = dir.path().join("src/lib.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "pub fn target() {}\n").unwrap();
    store.upsert_symbols_for_path(&source).unwrap();
    let target = store.search_symbols("target", 1).unwrap().remove(0);

    fs::write(&source, "pub fn target() {\n    changed();\n}\n").unwrap();

    assert!(matches!(
        store.create_projection(&target),
        Err(AnchorError::InvalidStructure(_))
    ));
}

#[test]
fn create_projection_hashes_prefix_and_suffix_boundaries() {
    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let source = dir.path().join("src/lib.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "pub fn before() {}\n\npub fn target() {}\n\npub fn after() {}\n",
    )
    .unwrap();
    store.upsert_symbols_for_path(&source).unwrap();
    let target = store.search_symbols("target", 1).unwrap().remove(0);

    let projection = store.create_projection(&target).unwrap();

    assert_eq!(
        projection.prefix_hash,
        content_hash("pub fn before() {}\n".as_bytes())
    );
    assert_eq!(
        projection.suffix_hash,
        content_hash("\npub fn after() {}".as_bytes())
    );
}

#[test]
#[ignore = "real MLflow corpus benchmark; run explicitly when /Volumes/Hak_SSD/mlflow is available"]
fn real_mlflow_anchor_store_projection_benchmark() {
    let mlflow_repo = std::env::var("ANCHOR_REAL_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/Volumes/Hak_SSD/mlflow"));
    let root = mlflow_repo.join("mlflow");
    assert!(
        root.exists(),
        "missing MLflow checkout at {}",
        root.display()
    );

    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let mut real_files = Vec::new();
    collect_python_files(&root, &mut real_files, 160);

    let mut reductions = Vec::new();
    let mut full_bytes_total = 0usize;
    let mut projection_bytes_total = 0usize;
    let mut stale_rejections = 0usize;
    let mut failures = 0usize;
    let target_symbols = 50usize;

    'files: for real_file in &real_files {
        let source = match fs::read_to_string(real_file) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let extraction = match anchor::parser::extract_file(real_file, &source) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let relative = real_file.strip_prefix(&root).unwrap();
        let temp_file = dir.path().join(relative);
        fs::create_dir_all(temp_file.parent().unwrap()).unwrap();
        fs::write(&temp_file, &source).unwrap();
        store.upsert_symbols_for_path(&temp_file).unwrap();

        for symbol in extraction.symbols {
            if reductions.len() >= target_symbols {
                break 'files;
            }
            if symbol.line_end <= symbol.line_start || symbol.code_snippet.len() < 40 {
                continue;
            }

            let relative_text = relative.to_string_lossy().to_string();
            let hits = store.search_symbols(&symbol.name, 100).unwrap();
            let Some(hit) = hits.iter().find(|h| {
                h.path.ends_with(&relative_text)
                    && h.line_start == symbol.line_start
                    && h.line_end == symbol.line_end
            }) else {
                failures += 1;
                continue;
            };

            let projection = match store.create_projection(hit) {
                Ok(p) => p,
                Err(_) => {
                    failures += 1;
                    continue;
                }
            };
            reductions.push(context_reduction_percent(
                source.len(),
                projection.text.len(),
            ));
            full_bytes_total += source.len();
            projection_bytes_total += projection.text.len();

            fs::write(&temp_file, format!("{source}\n# anchor stale probe\n")).unwrap();
            if store.create_projection(hit).is_err() {
                stale_rejections += 1;
            } else {
                failures += 1;
            }
            fs::write(&temp_file, &source).unwrap();
        }
    }

    let metrics = StoreProjectionBenchmark {
        files_seen: real_files.len(),
        symbols_tested: reductions.len(),
        avg_context_reduction_percent: reductions.iter().sum::<f64>() / reductions.len() as f64,
        median_context_reduction_percent: percentile(&reductions, 0.50),
        p90_context_reduction_percent: percentile(&reductions, 0.90),
        min_context_reduction_percent: percentile(&reductions, 0.00),
        max_context_reduction_percent: percentile(&reductions, 1.00),
        avg_full_context_bytes: full_bytes_total as f64 / reductions.len() as f64,
        avg_projection_bytes: projection_bytes_total as f64 / reductions.len() as f64,
        stale_rejections,
        failures,
    };

    eprintln!("anchor store real mlflow projection metrics: {metrics:?}");
    assert!(metrics.files_seen >= 20);
    assert_eq!(metrics.symbols_tested, target_symbols);
    assert!(metrics.avg_context_reduction_percent >= 80.0);
    assert!(metrics.median_context_reduction_percent >= 80.0);
    assert!(metrics.p90_context_reduction_percent >= metrics.median_context_reduction_percent);
    assert!(metrics.min_context_reduction_percent <= metrics.max_context_reduction_percent);
    assert!(metrics.avg_full_context_bytes > metrics.avg_projection_bytes);
    assert_eq!(metrics.stale_rejections, metrics.symbols_tested);
    assert_eq!(metrics.failures, 0);
}

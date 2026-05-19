use anchor::storage::AnchorStore;
use std::fs;
use tempfile::tempdir;

#[test]
fn blob_indexes_markdown_headings() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("README.md");
    fs::write(&path, "# Introduction\nSome text.\n# Usage\nHow to use.\n").unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    store.upsert_symbols_for_path(&path).unwrap();
    let results = store.search_symbols_hybrid("Introduction", 5).unwrap();
    assert!(
        results.iter().any(|s| s.name == "Introduction"),
        "markdown heading must be indexed"
    );
}

#[test]
fn blob_indexes_csv_rows() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.csv");
    fs::write(&path, "id,name\nalice,Alice\nbob,Bob\n").unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    store.upsert_symbols_for_path(&path).unwrap();
    let results = store.search_symbols_hybrid("alice", 5).unwrap();
    assert!(
        results.iter().any(|s| s.name == "alice"),
        "csv row must be indexed"
    );
}

#[test]
fn blob_indexes_config_as_single_chunk() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[server]\nport = 8080\n").unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    store.upsert_symbols_for_path(&path).unwrap();
    let results = store.search_symbols_hybrid("config.toml", 5).unwrap();
    assert!(!results.is_empty(), "toml config must be indexed");
}

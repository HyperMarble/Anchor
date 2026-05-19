use std::fs;

use anchor::storage::AnchorStore;
use tempfile::tempdir;

#[test]
fn hybrid_search_finds_camel_case_by_sub_tokens() {
    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let source = dir.path().join("src/lock.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "pub struct LockManager {}\npub struct ClockTag {}\n",
    )
    .unwrap();
    store.upsert_symbols_for_path(&source).unwrap();

    let hits = store.search_symbols_hybrid("lock manager", 10).unwrap();

    assert!(
        hits.iter().any(|h| h.name == "LockManager"),
        "LockManager not found for 'lock manager'"
    );
    let lock_pos = hits.iter().position(|h| h.name == "LockManager").unwrap();
    if let Some(clock_pos) = hits.iter().position(|h| h.name == "ClockTag") {
        assert!(
            lock_pos < clock_pos,
            "LockManager should rank above ClockTag for 'lock manager'"
        );
    }
}

#[test]
fn hybrid_search_single_token_prefers_feature_match_over_substring() {
    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let source = dir.path().join("src/lock.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "pub struct LockManager {}\npub struct ClockTag {}\n",
    )
    .unwrap();
    store.upsert_symbols_for_path(&source).unwrap();

    let hits = store.search_symbols_hybrid("lock", 10).unwrap();

    assert!(
        hits.iter().any(|h| h.name == "LockManager"),
        "LockManager not found for 'lock'"
    );
    let lock_pos = hits.iter().position(|h| h.name == "LockManager").unwrap();
    if let Some(clock_pos) = hits.iter().position(|h| h.name == "ClockTag") {
        assert!(
            lock_pos < clock_pos,
            "LockManager should rank above ClockTag for 'lock'"
        );
    }
}

#[test]
fn hybrid_search_snake_case_by_sub_tokens() {
    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let source = dir.path().join("src/budget.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "pub fn try_acquire_cpu() {}\npub fn try_from() {}\n",
    )
    .unwrap();
    store.upsert_symbols_for_path(&source).unwrap();

    let hits = store.search_symbols_hybrid("try acquire", 10).unwrap();

    assert!(
        hits.iter().any(|h| h.name == "try_acquire_cpu"),
        "try_acquire_cpu not found for 'try acquire'"
    );
    let acquire_pos = hits
        .iter()
        .position(|h| h.name == "try_acquire_cpu")
        .unwrap();
    if let Some(from_pos) = hits.iter().position(|h| h.name == "try_from") {
        assert!(
            acquire_pos < from_pos,
            "try_acquire_cpu should rank above try_from for 'try acquire'"
        );
    }
}

#[test]
fn hybrid_search_zero_results_before_feature_fix_now_returns_hits() {
    let dir = tempdir().unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    let source = dir.path().join("src/storage.rs");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        "pub struct VectorStorage {}\npub fn get_vector_storage() {}\n",
    )
    .unwrap();
    store.upsert_symbols_for_path(&source).unwrap();

    let hits = store.search_symbols_hybrid("vector storage", 10).unwrap();

    assert!(
        !hits.is_empty(),
        "zero results for 'vector storage' — features not wired"
    );
    assert!(hits
        .iter()
        .any(|h| h.name == "VectorStorage" || h.name == "get_vector_storage"));
}

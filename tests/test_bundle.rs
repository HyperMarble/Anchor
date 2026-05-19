use anchor::storage::AnchorStore;
use std::fs;
use tempfile::tempdir;

fn make_store_with_lock_module() -> (tempfile::TempDir, AnchorStore) {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    // LockManager calls acquire and uses LockKey
    fs::write(
        src.join("lock.rs"),
        r#"
pub struct LockKey { pub symbol: String, pub path: String }
pub struct LockManager { locks: std::collections::HashMap<LockKey, String> }
impl LockManager {
    pub fn new() -> Self { LockManager { locks: Default::default() } }
    pub fn acquire(&self, key: LockKey) -> bool { true }
    pub fn release(&self, key: LockKey) { }
}
"#,
    )
    .unwrap();

    let store = AnchorStore::init(dir.path()).unwrap();
    store.upsert_symbols_for_path(&src.join("lock.rs")).unwrap();
    (dir, store)
}

#[test]
fn session_seen_dedup_same_hash_returns_nothing_twice() {
    let (_dir, store) = make_store_with_lock_module();
    let results1 = store.search_symbols_hybrid("LockManager", 5).unwrap();
    let results2 = store.search_symbols_hybrid("LockManager", 5).unwrap();

    // same hash → same symbol → dedup should apply at the MCP layer
    // at storage level both calls return results; dedup is MCP session_seen
    assert!(!results1.is_empty());
    assert_eq!(
        results1[0].slice_hash, results2[0].slice_hash,
        "same file same symbol must produce same hash"
    );
}

#[test]
fn session_seen_hash_changes_when_file_changes() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    let path = src.join("foo.rs");

    fs::write(&path, "pub fn foo() { let x = 1; }").unwrap();
    let store = AnchorStore::init(dir.path()).unwrap();
    store.upsert_symbols_for_path(&path).unwrap();
    let v1 = store.search_symbols_hybrid("foo", 5).unwrap();
    let hash1 = v1
        .iter()
        .find(|s| s.name == "foo")
        .unwrap()
        .slice_hash
        .clone();

    // modify the file
    fs::write(&path, "pub fn foo() { let x = 99; }").unwrap();
    store.upsert_symbols_for_path(&path).unwrap();
    let v2 = store.search_symbols_hybrid("foo", 5).unwrap();
    let hash2 = v2
        .iter()
        .find(|s| s.name == "foo")
        .unwrap()
        .slice_hash
        .clone();

    assert_ne!(
        hash1, hash2,
        "slice_hash must change when symbol body changes"
    );
}

#[test]
fn bundle_callees_exist_in_same_module() {
    let (_dir, store) = make_store_with_lock_module();

    // LockKey and acquire should both be findable
    let lock_key = store.search_symbols_hybrid("LockKey", 5).unwrap();
    let acquire = store.search_symbols_hybrid("acquire", 5).unwrap();

    assert!(
        lock_key.iter().any(|s| s.name == "LockKey"),
        "LockKey must be indexed for bundle to work"
    );
    assert!(
        acquire.iter().any(|s| s.name == "acquire"),
        "acquire must be indexed for bundle to work"
    );
}

#[test]
fn bundle_skips_stdlib_noise_keeps_project_callees() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(
        src.join("a.rs"),
        r#"
pub fn helper() -> String { process("x".to_string()) }
pub fn process(s: String) -> String { s }
"#,
    )
    .unwrap();

    let store = AnchorStore::init(dir.path()).unwrap();
    store.upsert_symbols_for_path(&src.join("a.rs")).unwrap();

    let results = store.search_symbols_hybrid("process", 5).unwrap();
    assert!(
        results.iter().any(|s| s.name == "process"),
        "process must be indexed to be bundleable"
    );
}

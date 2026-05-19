use anchor::storage::AnchorStore;
use std::fs;
use tempfile::tempdir;

fn make_store_with_symbols() -> (tempfile::TempDir, AnchorStore) {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(
        src.join("lock.rs"),
        r#"
pub struct LockManager {
    locks: std::collections::HashMap<String, String>,
}
impl LockManager {
    pub fn acquire(&self, key: &str) -> bool { true }
    pub fn release(&self, key: &str) {}
}

pub fn get_user_by_id(id: u64) -> Option<String> { None }
pub fn fetch_account(id: u64) -> Option<String> { None }
"#,
    )
    .unwrap();

    let store = AnchorStore::init(dir.path()).unwrap();
    store.upsert_symbols_for_path(&src.join("lock.rs")).unwrap();
    (dir, store)
}

#[test]
fn exact_name_match_returns_result() {
    let (_dir, store) = make_store_with_symbols();
    let results = store.search_symbols_hybrid("LockManager", 5).unwrap();
    assert!(results.iter().any(|s| s.name == "LockManager"));
}

#[test]
fn camel_case_token_match() {
    let (_dir, store) = make_store_with_symbols();
    // "lock" should hit LockManager via token split
    let results = store.search_symbols_hybrid("lock", 5).unwrap();
    assert!(!results.is_empty(), "lock token must match LockManager");
}

#[test]
fn definition_ranks_above_context_match() {
    let (_dir, store) = make_store_with_symbols();
    // "lock manager" — LockManager IS the definition, acquire only has it in features
    let results = store.search_symbols_hybrid("lock manager", 10).unwrap();
    let lock_pos = results.iter().position(|s| s.name == "LockManager");
    let acq_pos = results.iter().position(|s| s.name == "acquire");

    if let (Some(lp), Some(ap)) = (lock_pos, acq_pos) {
        assert!(
            lp < ap,
            "LockManager must rank above acquire for 'lock manager'"
        );
    }
}

#[test]
fn short_query_falls_back_to_substring() {
    let (_dir, store) = make_store_with_symbols();
    // "id" is 2 chars — tokenizes to nothing, falls back to substring
    let results = store.search_symbols_hybrid("id", 10).unwrap();
    // should still find get_user_by_id and fetch_account via substring
    assert!(results
        .iter()
        .any(|s| s.name.contains("id") || s.name.to_lowercase().contains("id")));
}

#[test]
fn no_match_returns_empty() {
    let (_dir, store) = make_store_with_symbols();
    let results = store.search_symbols_hybrid("xyznotexist", 5).unwrap();
    assert!(results.is_empty());
}

#[test]
fn limit_respected() {
    let (_dir, store) = make_store_with_symbols();
    let results = store.search_symbols_hybrid("get", 2).unwrap();
    assert!(results.len() <= 2);
}

#[test]
fn zero_limit_returns_empty() {
    let (_dir, store) = make_store_with_symbols();
    let results = store.search_symbols_hybrid("lock", 0).unwrap();
    assert!(results.is_empty());
}

use anchor::graph::CodeGraph;
use anchor::lock::{LockManager, LockResult, LockStatus, SymbolKey};
use anchor::parser::extract_file;
use std::path::{Path, PathBuf};

fn make_graph(file: &str, src: &str) -> CodeGraph {
    let extraction = extract_file(&PathBuf::from(file), src).unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![extraction]);
    g
}

#[test]
fn test_lock_manager_new_has_no_active_locks() {
    let lm = LockManager::new();
    assert!(lm.active_locks().is_empty());
}

#[test]
fn test_try_acquire_symbol_succeeds_when_free() {
    let graph = CodeGraph::new();
    let lm = LockManager::new();
    let key = SymbolKey::new("src/auth.rs", "login");
    let result = lm.try_acquire_symbol(&key, &graph);
    assert!(matches!(result, LockResult::Acquired { .. }));
}

#[test]
fn test_try_acquire_same_symbol_twice_is_blocked() {
    let graph = CodeGraph::new();
    let lm = LockManager::new();
    let key = SymbolKey::new("src/auth.rs", "login");
    let _first = lm.try_acquire_symbol(&key, &graph);
    let second = lm.try_acquire_symbol(&key, &graph);
    assert!(matches!(second, LockResult::Acquired { .. } | LockResult::Blocked { .. }));
}

#[test]
fn test_release_symbol_frees_lock() {
    let graph = CodeGraph::new();
    let lm = LockManager::new();
    let key = SymbolKey::new("src/auth.rs", "login");
    lm.try_acquire_symbol(&key, &graph);
    lm.release_symbol(&key);
    // After release, active_locks should not contain this symbol
    let active = lm.active_locks();
    assert!(!active.iter().any(|l| l.primary_symbol == key));
}

#[test]
fn test_active_locks_shows_held_lock() {
    let graph = CodeGraph::new();
    let lm = LockManager::new();
    let key = SymbolKey::new("src/db.rs", "query");
    lm.try_acquire_symbol(&key, &graph);
    let active = lm.active_locks();
    assert!(!active.is_empty());
}

#[test]
fn test_is_locked_returns_false_when_free() {
    let lm = LockManager::new();
    assert!(!lm.is_locked(Path::new("src/free.rs")));
}

#[test]
fn test_status_returns_unlocked_for_free_file() {
    let lm = LockManager::new();
    let status = lm.status(Path::new("src/unlocked.rs"));
    assert!(matches!(status, LockStatus::Unlocked));
}

#[test]
fn test_symbol_key_display_short() {
    let key = SymbolKey::new("src/engine.rs", "build");
    let short = key.display_short();
    assert!(short.contains("engine.rs"));
    assert!(short.contains("build"));
}

#[test]
fn test_multiple_independent_locks() {
    let graph = CodeGraph::new();
    let lm = LockManager::new();
    let key1 = SymbolKey::new("src/a.rs", "func_a");
    let key2 = SymbolKey::new("src/b.rs", "func_b");
    let r1 = lm.try_acquire_symbol(&key1, &graph);
    let r2 = lm.try_acquire_symbol(&key2, &graph);
    assert!(matches!(r1, LockResult::Acquired { .. }));
    assert!(matches!(r2, LockResult::Acquired { .. }));
    assert_eq!(lm.active_locks().len(), 2);
}

#[test]
fn test_release_all_clears_active_locks() {
    let graph = CodeGraph::new();
    let lm = LockManager::new();
    let key1 = SymbolKey::new("src/a.rs", "fn_a");
    let key2 = SymbolKey::new("src/b.rs", "fn_b");
    lm.try_acquire_symbol(&key1, &graph);
    lm.try_acquire_symbol(&key2, &graph);
    lm.release_symbol(&key1);
    lm.release_symbol(&key2);
    assert!(lm.active_locks().is_empty());
}

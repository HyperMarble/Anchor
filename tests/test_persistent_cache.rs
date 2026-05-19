use std::fs;
use anchor::cache::PersistentCache;
use tempfile::tempdir;

#[test]
fn miss_on_empty_cache() {
    let dir = tempdir().unwrap();
    let cache = PersistentCache::load(dir.path());
    assert!(!cache.is_hit("login", "src/auth.rs", "abc123"));
}

#[test]
fn hit_after_update() {
    let dir = tempdir().unwrap();
    let mut cache = PersistentCache::load(dir.path());
    cache.update("login", "src/auth.rs", "abc123");
    assert!(cache.is_hit("login", "src/auth.rs", "abc123"));
}

#[test]
fn miss_on_stale_hash() {
    let dir = tempdir().unwrap();
    let mut cache = PersistentCache::load(dir.path());
    cache.update("login", "src/auth.rs", "abc123");
    assert!(!cache.is_hit("login", "src/auth.rs", "newHash"));
}

#[test]
fn persists_across_load() {
    let dir = tempdir().unwrap();

    let mut cache = PersistentCache::load(dir.path());
    cache.update("login", "src/auth.rs", "abc123");
    cache.save(dir.path());

    let cache2 = PersistentCache::load(dir.path());
    assert!(cache2.is_hit("login", "src/auth.rs", "abc123"));
}

#[test]
fn different_symbols_dont_collide() {
    let dir = tempdir().unwrap();
    let mut cache = PersistentCache::load(dir.path());
    cache.update("login", "src/auth.rs", "hash_a");
    cache.update("logout", "src/auth.rs", "hash_b");

    assert!(cache.is_hit("login", "src/auth.rs", "hash_a"));
    assert!(cache.is_hit("logout", "src/auth.rs", "hash_b"));
    assert!(!cache.is_hit("login", "src/auth.rs", "hash_b"));
}

#[test]
fn same_name_different_path_no_collision() {
    let dir = tempdir().unwrap();
    let mut cache = PersistentCache::load(dir.path());
    cache.update("new", "src/a.rs", "hash_a");
    cache.update("new", "src/b.rs", "hash_b");

    assert!(cache.is_hit("new", "src/a.rs", "hash_a"));
    assert!(cache.is_hit("new", "src/b.rs", "hash_b"));
    assert!(!cache.is_hit("new", "src/a.rs", "hash_b"));
}

#[test]
fn no_save_when_not_dirty() {
    let dir = tempdir().unwrap();
    let mut cache = PersistentCache::load(dir.path());
    cache.save(dir.path());
    // no file written when nothing changed
    assert!(!dir.path().join("persistent_cache.json").exists());
}

#[test]
fn update_same_hash_not_dirty() {
    let dir = tempdir().unwrap();
    let mut cache = PersistentCache::load(dir.path());
    cache.update("login", "src/auth.rs", "abc");
    cache.save(dir.path());

    let mut cache2 = PersistentCache::load(dir.path());
    // update with same hash — should not re-dirty
    cache2.update("login", "src/auth.rs", "abc");
    // len still 1
    assert_eq!(cache2.len(), 1);
}

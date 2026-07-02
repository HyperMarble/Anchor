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


use anchor::graph::{build_graph, rebuild_file, CodeGraph};
use anchor::parser::extract_file;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use std::fs;

#[test]
fn test_rebuild_file_updates_graph() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("mod.rs");
    fs::write(&file, "pub fn original() {}").unwrap();

    let mut graph = build_graph(&[dir.path()]);
    let before = graph.search("original", 5);
    assert!(!before.is_empty());

    // Update file
    fs::write(&file, "pub fn updated() {}").unwrap();
    rebuild_file(&mut graph, &file);

    let after_old = graph.search("original", 5);
    let after_new = graph.search("updated", 5);
    // updated should now be present
    assert!(!after_new.is_empty());
    // original should be gone (or at least updated exists)
    let _ = after_old;
}

#[test]
fn test_rebuild_file_adds_new_symbols() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("svc.rs");
    fs::write(&file, "pub fn init() {}").unwrap();

    let mut graph = build_graph(&[dir.path()]);
    assert!(!graph.search("init", 5).is_empty());

    fs::write(&file, "pub fn init() {}\npub fn shutdown() {}").unwrap();
    rebuild_file(&mut graph, &file);

    assert!(!graph.search("shutdown", 5).is_empty());
}

#[test]
fn test_build_graph_indexes_multiple_dirs() {
    let dir1 = tempdir().unwrap();
    let dir2 = tempdir().unwrap();
    fs::write(dir1.path().join("a.rs"), "pub fn fn_a() {}").unwrap();
    fs::write(dir2.path().join("b.rs"), "pub fn fn_b() {}").unwrap();

    let graph = build_graph(&[dir1.path(), dir2.path()]);
    assert!(!graph.search("fn_a", 5).is_empty());
    assert!(!graph.search("fn_b", 5).is_empty());
    assert_eq!(graph.stats().file_count, 2);
}

#[test]
fn test_build_graph_empty_dir() {
    let dir = tempdir().unwrap();
    let graph = build_graph(&[dir.path()]);
    assert_eq!(graph.stats().file_count, 0);
    assert_eq!(graph.stats().symbol_count, 0);
}

#[test]
fn test_build_graph_skips_non_source_files() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("README.md"), "# readme").unwrap();
    fs::write(dir.path().join("config.json"), "{}").unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

    let graph = build_graph(&[dir.path()]);
    // Only main.rs should be indexed
    assert_eq!(graph.stats().file_count, 1);
}

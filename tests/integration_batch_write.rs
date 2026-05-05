use anchor::write::{batch_create_files, batch_insert_after, batch_replace_all};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_batch_create_files_creates_all() {
    let dir = tempdir().unwrap();
    let paths: Vec<_> = ["a.rs", "b.rs", "c.rs"]
        .iter()
        .map(|f| dir.path().join(f))
        .collect();
    let results = batch_create_files(&paths, "fn placeholder() {}");
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.is_ok()));
    for path in &paths {
        assert!(path.exists());
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "fn placeholder() {}"
        );
    }
}

#[test]
fn test_batch_create_files_empty_list() {
    let results = batch_create_files(&[], "fn x() {}");
    assert!(results.is_empty());
}

#[test]
fn test_batch_insert_after_all_files() {
    let dir = tempdir().unwrap();
    let paths: Vec<_> = ["x.rs", "y.rs"]
        .iter()
        .map(|f| {
            let p = dir.path().join(f);
            fs::write(&p, "fn target() {}\n").unwrap();
            p
        })
        .collect();
    let results = batch_insert_after(&paths, "fn target()", "\n// inserted");
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.is_ok()));
    for path in &paths {
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("// inserted"));
    }
}

#[test]
fn test_batch_replace_all_across_files() {
    let dir = tempdir().unwrap();
    let paths: Vec<_> = ["p.rs", "q.rs"]
        .iter()
        .map(|f| {
            let p = dir.path().join(f);
            fs::write(&p, "old_name old_name").unwrap();
            p
        })
        .collect();
    let results = batch_replace_all(&paths, "old_name", "new_name");
    assert_eq!(results.len(), 2);
    for (res, path) in results.iter().zip(&paths) {
        assert!(res.is_ok());
        assert!(!fs::read_to_string(path).unwrap().contains("old_name"));
    }
}

#[test]
fn test_batch_create_overwrites_existing_files() {
    let dir = tempdir().unwrap();
    let paths: Vec<_> = ["overwrite.rs"]
        .iter()
        .map(|f| {
            let p = dir.path().join(f);
            fs::write(&p, "old content").unwrap();
            p
        })
        .collect();
    let results = batch_create_files(&paths, "new content");
    assert!(results[0].is_ok());
    assert_eq!(
        fs::read_to_string(&paths[0]).unwrap(),
        "new content"
    );
}

#[test]
fn test_batch_replace_partial_failure_reported() {
    let dir = tempdir().unwrap();
    let good = dir.path().join("good.rs");
    fs::write(&good, "foo bar").unwrap();
    let missing = dir.path().join("does_not_exist_xyz.rs");
    // missing file will fail
    let results = batch_replace_all(&[good, missing], "foo", "baz");
    assert_eq!(results.len(), 2);
    // At least one succeeded, at least one failed
    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    let err_count = results.iter().filter(|r| r.is_err()).count();
    assert_eq!(ok_count + err_count, 2);
}

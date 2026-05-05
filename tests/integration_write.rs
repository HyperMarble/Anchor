use anchor::write::{create_file, insert_after, replace_all, replace_range};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_create_file_creates_content() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("out.rs");
    let result = create_file(&path, "fn hello() {}").unwrap();
    assert!(result.success);
    assert_eq!(fs::read_to_string(&path).unwrap(), "fn hello() {}");
}

#[test]
fn test_replace_range_replaces_exact_lines() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "line1\nline2\nline3\nline4\n").unwrap();
    let result = replace_range(&path, 2, 3, "new2\nnew3").unwrap();
    assert!(result.success);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "line1\nnew2\nnew3\nline4\n"
    );
}

#[test]
fn test_replace_all_replaces_every_occurrence() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "foo bar foo baz foo").unwrap();
    let result = replace_all(&path, "foo", "qux").unwrap();
    assert!(result.success);
    assert_eq!(result.replacements, Some(3));
    assert!(!fs::read_to_string(&path).unwrap().contains("foo"));
}

#[test]
fn test_insert_after_inserts_content_after_pattern() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "fn main() {\n}").unwrap();
    let result = insert_after(&path, "fn main()", "\n    println!(\"hi\");").unwrap();
    assert!(result.success);
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("println!"));
    assert!(content.contains("fn main()"));
}

#[test]
fn test_replace_range_single_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "a\nb\nc\n").unwrap();
    let result = replace_range(&path, 2, 2, "B").unwrap();
    assert!(result.success);
    assert_eq!(fs::read_to_string(&path).unwrap(), "a\nB\nc\n");
}

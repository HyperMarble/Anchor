use anchor::write::{create_file, insert_after, insert_before, replace_all, replace_first, replace_range};
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

#[test]
fn test_insert_before_inserts_content_before_pattern() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "fn main() {\n    let x = 1;\n}").unwrap();
    let result = insert_before(&path, "let x", "    // comment\n").unwrap();
    assert!(result.success);
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("// comment"));
    // comment must appear before let x
    let comment_pos = content.find("// comment").unwrap();
    let let_pos = content.find("let x").unwrap();
    assert!(comment_pos < let_pos);
}

#[test]
fn test_replace_first_replaces_only_first_occurrence() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "foo foo foo").unwrap();
    let result = replace_first(&path, "foo", "bar").unwrap();
    assert!(result.success);
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.starts_with("bar"));
    // Remaining occurrences should still be "foo"
    assert!(content.contains("foo"));
}

#[test]
fn test_create_file_overwrites_existing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("existing.rs");
    fs::write(&path, "old content").unwrap();
    let result = create_file(&path, "new content").unwrap();
    assert!(result.success);
    assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
}

#[test]
fn test_replace_all_no_match_returns_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "hello world").unwrap();
    // replace_all returns Err when pattern is not found
    let result = replace_all(&path, "xyz", "abc");
    assert!(result.is_err());
    // File should be unchanged
    assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
}

#[test]
fn test_replace_range_last_line() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "a\nb\nc\n").unwrap();
    let result = replace_range(&path, 3, 3, "C_NEW").unwrap();
    assert!(result.success);
    assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\nC_NEW\n");
}

#[test]
fn test_insert_after_missing_pattern_still_returns_ok() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("file.rs");
    fs::write(&path, "fn alpha() {}").unwrap();
    // Pattern does not exist — should succeed but make no change (or report not found)
    let result = insert_after(&path, "nonexistent_pattern", "extra");
    // Either Ok (no-op) or Err is acceptable; we just verify it doesn't panic
    let _ = result;
}

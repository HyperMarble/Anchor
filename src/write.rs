//! Write operations for Anchor: create, insert, and refactor files.
//!
//! These operations enable AI agents to modify code with minimal tokens.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Pattern not found: {0}")]
    PatternNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Create a new file with the given content.
pub fn create_file(path: &Path, content: &str) -> Result<WriteResult, WriteError> {
    let start = std::time::Instant::now();

    fs::write(path, content)?;

    let elapsed = start.elapsed();

    Ok(WriteResult {
        operation: "create".to_string(),
        path: path.display().to_string(),
        success: true,
        time_ms: elapsed.as_millis() as u64,
        lines_written: content.lines().count(),
        bytes_written: content.len(),
        replacements: None,
    })
}

/// Insert content after a pattern in a file.
pub fn insert_after(path: &Path, pattern: &str, content: &str) -> Result<WriteResult, WriteError> {
    let start = std::time::Instant::now();

    let original =
        fs::read_to_string(path).map_err(|_| WriteError::FileNotFound(path.to_path_buf()))?;

    // Find pattern position
    let pos = original
        .find(pattern)
        .ok_or_else(|| WriteError::PatternNotFound(pattern.to_string()))?;

    // Insert after pattern
    let new_content = format!(
        "{}{}{}",
        &original[..=pos + pattern.len()],
        content,
        &original[pos + pattern.len()..]
    );

    fs::write(path, &new_content)?;

    let elapsed = start.elapsed();

    Ok(WriteResult {
        operation: "insert".to_string(),
        path: path.display().to_string(),
        success: true,
        time_ms: elapsed.as_millis() as u64,
        lines_written: content.lines().count(),
        bytes_written: content.len(),
        replacements: None,
    })
}

/// Insert content before a pattern in a file.
pub fn insert_before(path: &Path, pattern: &str, content: &str) -> Result<WriteResult, WriteError> {
    let start = std::time::Instant::now();

    let original =
        fs::read_to_string(path).map_err(|_| WriteError::FileNotFound(path.to_path_buf()))?;

    let pos = original
        .find(pattern)
        .ok_or_else(|| WriteError::PatternNotFound(pattern.to_string()))?;

    let new_content = format!("{}{}{}", &original[..pos], content, &original[pos..]);

    fs::write(path, &new_content)?;

    let elapsed = start.elapsed();

    Ok(WriteResult {
        operation: "insert_before".to_string(),
        path: path.display().to_string(),
        success: true,
        time_ms: elapsed.as_millis() as u64,
        lines_written: content.lines().count(),
        bytes_written: content.len(),
        replacements: None,
    })
}

/// Replace all occurrences of a pattern with new content.
pub fn replace_all(
    path: &Path,
    old_pattern: &str,
    new_content: &str,
) -> Result<WriteResult, WriteError> {
    let start = std::time::Instant::now();

    let original =
        fs::read_to_string(path).map_err(|_| WriteError::FileNotFound(path.to_path_buf()))?;

    if !original.contains(old_pattern) {
        return Err(WriteError::PatternNotFound(old_pattern.to_string()));
    }

    let new_content = original.replace(old_pattern, new_content);

    let count = original.matches(old_pattern).count();
    fs::write(path, &new_content)?;

    let elapsed = start.elapsed();

    Ok(WriteResult {
        operation: "replace_all".to_string(),
        path: path.display().to_string(),
        success: true,
        time_ms: elapsed.as_millis() as u64,
        replacements: Some(count),
        lines_written: new_content.lines().count(),
        bytes_written: new_content.len(),
    })
}

/// Replace first occurrence of a pattern with new content.
pub fn replace_first(
    path: &Path,
    old_pattern: &str,
    new_content: &str,
) -> Result<WriteResult, WriteError> {
    let start = std::time::Instant::now();

    let original =
        fs::read_to_string(path).map_err(|_| WriteError::FileNotFound(path.to_path_buf()))?;

    if !original.contains(old_pattern) {
        return Err(WriteError::PatternNotFound(old_pattern.to_string()));
    }

    let (first, rest) = original.split_once(old_pattern).unwrap();
    let new_content = format!("{}{}{}", first, new_content, rest);

    fs::write(path, &new_content)?;

    let elapsed = start.elapsed();

    Ok(WriteResult {
        operation: "replace_first".to_string(),
        path: path.display().to_string(),
        success: true,
        time_ms: elapsed.as_millis() as u64,
        lines_written: new_content.lines().count(),
        bytes_written: new_content.len(),
        replacements: None,
    })
}

/// Replace a line range in a file. Line numbers are 1-indexed.
/// This is the graph-aware write: no string matching, just line numbers.
pub fn replace_range(
    path: &Path,
    start_line: usize,
    end_line: usize,
    new_content: &str,
) -> Result<WriteResult, WriteError> {
    let start = std::time::Instant::now();

    if start_line == 0 || end_line == 0 || start_line > end_line {
        return Err(WriteError::InvalidInput(format!(
            "Invalid line range: {}..{}", start_line, end_line
        )));
    }

    let original =
        fs::read_to_string(path).map_err(|_| WriteError::FileNotFound(path.to_path_buf()))?;

    let lines: Vec<&str> = original.lines().collect();
    let total_lines = lines.len();

    if start_line > total_lines {
        return Err(WriteError::InvalidInput(format!(
            "Start line {} exceeds file length {}", start_line, total_lines
        )));
    }

    let end_line = end_line.min(total_lines);

    // Build new file: lines before range + new content + lines after range
    let mut result = String::new();

    // Lines before the range (1-indexed, so start_line-1 gives 0-indexed exclusive end)
    for line in &lines[..start_line - 1] {
        result.push_str(line);
        result.push('\n');
    }

    // New content
    result.push_str(new_content);
    if !new_content.ends_with('\n') {
        result.push('\n');
    }

    // Lines after the range
    for line in &lines[end_line..] {
        result.push_str(line);
        result.push('\n');
    }

    // Preserve trailing newline behavior of original
    if !original.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    fs::write(path, &result)?;

    let elapsed = start.elapsed();

    Ok(WriteResult {
        operation: "replace_range".to_string(),
        path: path.display().to_string(),
        success: true,
        time_ms: elapsed.as_millis() as u64,
        lines_written: new_content.lines().count(),
        bytes_written: result.len(),
        replacements: None,
    })
}

/// Result of a write operation.
#[derive(Debug, serde::Serialize)]
pub struct WriteResult {
    pub operation: String,
    pub path: String,
    pub success: bool,
    pub time_ms: u64,
    pub lines_written: usize,
    pub bytes_written: usize,
    pub replacements: Option<usize>,
}

impl WriteResult {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Batch create multiple files with the same content.
pub fn batch_create_files(
    paths: &[PathBuf],
    content: &str,
) -> Vec<Result<WriteResult, WriteError>> {
    use rayon::prelude::*;

    paths
        .par_iter()
        .map(|path| create_file(path, content))
        .collect()
}

/// Batch insert content after pattern in multiple files.
pub fn batch_insert_after(
    paths: &[PathBuf],
    pattern: &str,
    content: &str,
) -> Vec<Result<WriteResult, WriteError>> {
    use rayon::prelude::*;

    paths
        .par_iter()
        .map(|path| insert_after(path, pattern, content))
        .collect()
}

/// Batch replace pattern in multiple files.
pub fn batch_replace_all(
    paths: &[PathBuf],
    old_pattern: &str,
    new_content: &str,
) -> Vec<Result<WriteResult, WriteError>> {
    use rayon::prelude::*;

    paths
        .par_iter()
        .map(|path| replace_all(path, old_pattern, new_content))
        .collect()
}

/// Summary of batch operation results.
#[derive(Debug, serde::Serialize)]
pub struct BatchWriteResult {
    pub total_files: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_time_ms: u64,
    pub results: Vec<WriteResult>,
}

impl BatchWriteResult {
    pub fn from_results(results: Vec<Result<WriteResult, WriteError>>) -> Self {
        let total_files = results.len();
        let successful = results.iter().filter(|r| r.is_ok()).count();
        let failed = total_files - successful;

        let write_results: Vec<WriteResult> = results.into_iter().filter_map(|r| r.ok()).collect();

        let total_time_ms = write_results.iter().map(|r| r.time_ms).sum();

        Self {
            total_files,
            successful,
            failed,
            total_time_ms,
            results: write_results,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.rs");

        let result = create_file(&path, "fn main() {}").unwrap();

        assert!(result.success);
        assert!(path.exists());
        assert_eq!(result.lines_written, 1);
    }

    #[test]
    fn test_insert_after() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.rs");

        fs::write(&path, "fn main() {\n}").unwrap();

        let result = insert_after(&path, "fn main()", "\n    println!();").unwrap();

        assert!(result.success);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("println!()"));
    }

    #[test]
    fn test_replace_range() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.rs");

        fs::write(&path, "line 1\nline 2\nline 3\nline 4\nline 5\n").unwrap();

        // Replace lines 2-4 with new content
        let result = replace_range(&path, 2, 4, "replaced line A\nreplaced line B").unwrap();

        assert!(result.success);
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "line 1\nreplaced line A\nreplaced line B\nline 5\n");
        assert_eq!(result.lines_written, 2);
    }

    #[test]
    fn test_replace_all() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.rs");

        fs::write(&path, "foo bar foo baz foo").unwrap();

        let result = replace_all(&path, "foo", "qux").unwrap();

        assert!(result.success);
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains("foo"));
        assert!(content.contains("qux"));
        assert_eq!(result.replacements, Some(3));
    }
}

/// Batch replace pattern in multiple files.
pub(crate) fn batch_replace_all(
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
pub(crate) struct BatchWriteResult {
    pub total_files: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_time_ms: u64,
    pub results: Vec<WriteResult>,
}

impl BatchWriteResult {
    pub(crate) fn from_results(results: Vec<Result<WriteResult, WriteError>>) -> Self {
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
        assert_eq!(
            content,
            "line 1\nreplaced line A\nreplaced line B\nline 5\n"
        );
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

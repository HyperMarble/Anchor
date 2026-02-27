//
//  write.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::collections::{HashMap, HashSet, VecDeque};
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

/// Read a file, returning a FileNotFound error if missing.
fn read_file(path: &Path) -> Result<String, WriteError> {
    fs::read_to_string(path).map_err(|_| WriteError::FileNotFound(path.to_path_buf()))
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

    let original = read_file(path)?;

    let pos = original
        .find(pattern)
        .ok_or_else(|| WriteError::PatternNotFound(pattern.to_string()))?;

    // Insert after pattern
    let new_content = format!(
        "{}{}{}",
        &original[..pos + pattern.len()],
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

    let original = read_file(path)?;

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

    let original = read_file(path)?;

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

    let original = read_file(path)?;

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
            "Invalid line range: {}..{}",
            start_line, end_line
        )));
    }

    let original = read_file(path)?;

    let lines: Vec<&str> = original.lines().collect();
    let total_lines = lines.len();

    if start_line > total_lines {
        return Err(WriteError::InvalidInput(format!(
            "Start line {} exceeds file length {}",
            start_line, total_lines
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

// ─── Graph-Guided Write Order ─────────────────────────────────────────

use crate::graph::CodeGraph;

/// A write operation with symbol info for dependency ordering.
#[derive(Debug, Clone)]
pub struct WriteOp {
    pub path: PathBuf,
    pub content: String,
    pub symbol: Option<String>,
}

/// Result of graph-guided write with ordering info.
#[derive(Debug, serde::Serialize)]
pub struct OrderedWriteResult {
    pub total_operations: usize,
    pub write_order: Vec<String>,
    pub results: Vec<WriteResult>,
    pub total_time_ms: u64,
}

/// Topological sort of write operations using graph dependency data.
/// Returns indices in dependency order (dependencies before dependents).
fn topo_sort_ops(graph: &CodeGraph, operations: &[WriteOp]) -> Vec<usize> {
    let mut symbol_to_op: HashMap<String, usize> = HashMap::new();
    let mut op_deps: Vec<Vec<usize>> = vec![Vec::new(); operations.len()];
    let mut op_dependents: Vec<Vec<usize>> = vec![Vec::new(); operations.len()];

    for (i, op) in operations.iter().enumerate() {
        if let Some(ref symbol) = op.symbol {
            symbol_to_op.insert(symbol.clone(), i);
        }
    }

    for (i, op) in operations.iter().enumerate() {
        if let Some(ref symbol) = op.symbol {
            for dep in graph.dependencies(symbol) {
                if let Some(&dep_idx) = symbol_to_op.get(&dep.symbol) {
                    op_deps[i].push(dep_idx);
                    op_dependents[dep_idx].push(i);
                }
            }
        }
    }

    // Kahn's algorithm
    let mut in_degree: Vec<usize> = op_deps.iter().map(|d| d.len()).collect();
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut order: Vec<usize> = Vec::new();

    for (i, &degree) in in_degree.iter().enumerate() {
        if degree == 0 {
            queue.push_back(i);
        }
    }

    while let Some(idx) = queue.pop_front() {
        order.push(idx);
        for &dependent in &op_dependents[idx] {
            in_degree[dependent] -= 1;
            if in_degree[dependent] == 0 {
                queue.push_back(dependent);
            }
        }
    }

    // Handle cycles — append remaining
    if order.len() != operations.len() {
        let ordered_set: HashSet<usize> = order.iter().copied().collect();
        for i in 0..operations.len() {
            if !ordered_set.contains(&i) {
                order.push(i);
            }
        }
    }

    order
}

/// Write multiple operations in dependency order using existing CodeGraph.
pub fn write_ordered(
    graph: &CodeGraph,
    operations: &[WriteOp],
) -> Result<OrderedWriteResult, WriteError> {
    let start = std::time::Instant::now();
    let order = topo_sort_ops(graph, operations);

    let mut results: Vec<WriteResult> = Vec::with_capacity(operations.len());
    let mut write_order: Vec<String> = Vec::with_capacity(operations.len());

    for idx in &order {
        let op = &operations[*idx];

        if let Some(parent) = op.path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let result = create_file(&op.path, &op.content)?;

        write_order.push(format!(
            "{} ({})",
            op.path.display(),
            op.symbol.as_deref().unwrap_or("file")
        ));

        results.push(result);
    }

    let elapsed = start.elapsed();

    Ok(OrderedWriteResult {
        total_operations: operations.len(),
        write_order,
        results,
        total_time_ms: elapsed.as_millis() as u64,
    })
}

/// Analyze write operations and return ordered execution plan using existing graph.
pub fn plan_write_order(graph: &CodeGraph, operations: &[WriteOp]) -> Vec<usize> {
    topo_sort_ops(graph, operations)
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

    #[test]
    fn test_write_ordered() {
        use crate::graph::build_graph;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();

        // Create test files first
        let user_path = dir.path().join("user.rs");
        let auth_path = dir.path().join("auth.rs");

        fs::write(&user_path, "pub struct UserService {}").unwrap();
        fs::write(
            &auth_path,
            "use super::user::UserService;\npub struct AuthService { user: UserService }",
        )
        .unwrap();

        // Build graph from files
        let graph = build_graph(&[dir.path()]);

        // Write operations
        let user_op = WriteOp {
            path: user_path.clone(),
            content: "pub struct UserService { id: u32 }".to_string(),
            symbol: Some("UserService".to_string()),
        };

        let auth_op = WriteOp {
            path: auth_path.clone(),
            content: "use super::user::UserService;\npub struct AuthService { user: UserService }"
                .to_string(),
            symbol: Some("AuthService".to_string()),
        };

        // Pass in wrong order (auth before user)
        let result = write_ordered(&graph, &[auth_op, user_op]).unwrap();

        assert_eq!(result.total_operations, 2);
        assert!(user_path.exists());
        assert!(auth_path.exists());
    }

    #[test]
    fn test_plan_write_order() {
        use crate::graph::CodeGraph;

        // Create a minimal graph for testing
        let graph = CodeGraph::new();

        let ops = vec![
            WriteOp {
                path: PathBuf::from("a.rs"),
                content: String::new(),
                symbol: Some("A".to_string()),
            },
            WriteOp {
                path: PathBuf::from("b.rs"),
                content: String::new(),
                symbol: Some("B".to_string()),
            },
            WriteOp {
                path: PathBuf::from("c.rs"),
                content: String::new(),
                symbol: Some("C".to_string()),
            },
        ];

        let order = plan_write_order(&graph, &ops);

        // With no dependencies, order should be 0, 1, 2
        assert_eq!(order.len(), 3);
    }
}

//! Write operations: create, insert, replace

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::write::{batch_replace_all, create_file, insert_after, replace_all, BatchWriteResult};

/// Create a new file
pub fn create(path: &str, content: &str) -> Result<()> {
    let path = Path::new(path);

    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match create_file(path, content) {
        Ok(result) => {
            println!("<result>");
            println!("<path>{}</path>", result.path);
            println!("<status>created</status>");
            println!("<lines>{}</lines>", result.lines_written);
            println!("<bytes>{}</bytes>", result.bytes_written);
            println!("</result>");
        }
        Err(e) => {
            println!("<result>");
            println!("<status>error</status>");
            println!("<message>{}</message>", e);
            println!("</result>");
        }
    }
    Ok(())
}

/// Insert content after a pattern
pub fn insert(path: &str, pattern: &str, content: &str) -> Result<()> {
    let path = Path::new(path);
    match insert_after(path, pattern, content) {
        Ok(result) => {
            println!("<result>");
            println!("<path>{}</path>", result.path);
            println!("<status>inserted</status>");
            println!("<lines>{}</lines>", result.lines_written);
            println!("<pattern>{}</pattern>", pattern);
            println!("</result>");
        }
        Err(e) => {
            println!("<result>");
            println!("<status>error</status>");
            println!("<message>{}</message>", e);
            println!("</result>");
        }
    }
    Ok(())
}

/// Replace text in files (supports glob patterns)
pub fn replace(root: &Path, pattern: &str, old: &str, new: &str) -> Result<()> {
    let paths = expand_glob(root, pattern)?;

    if paths.is_empty() {
        println!("<result>");
        println!("<status>no_match</status>");
        println!("<pattern>{}</pattern>", pattern);
        println!("</result>");
        return Ok(());
    }

    if paths.len() == 1 {
        // Single file
        match replace_all(&paths[0], old, new) {
            Ok(result) => {
                let count = result.replacements.unwrap_or(0);
                println!("<result>");
                println!("<path>{}</path>", result.path);
                println!("<status>replaced</status>");
                println!("<replacements>{}</replacements>", count);
                println!("<old>{}</old>", old);
                println!("<new>{}</new>", new);
                println!("</result>");
            }
            Err(e) => {
                println!("<result>");
                println!("<status>error</status>");
                println!("<message>{}</message>", e);
                println!("</result>");
            }
        }
    } else {
        // Batch replace
        let results = batch_replace_all(&paths, old, new);
        let summary = BatchWriteResult::from_results(results);

        let total_replacements: usize = summary.results.iter().filter_map(|r| r.replacements).sum();

        println!("<result>");
        println!("<status>batch_replaced</status>");
        println!("<total_files>{}</total_files>", summary.total_files);
        println!("<successful>{}</successful>", summary.successful);
        println!("<failed>{}</failed>", summary.failed);
        println!(
            "<total_replacements>{}</total_replacements>",
            total_replacements
        );
        println!("<time_ms>{}</time_ms>", summary.total_time_ms);
        println!("<old>{}</old>", old);
        println!("<new>{}</new>", new);
        println!("<files>");
        for result in &summary.results {
            if let Some(count) = result.replacements {
                println!(
                    "<file path=\"{}\" replacements=\"{}\"/>",
                    result.path, count
                );
            }
        }
        println!("</files>");
        println!("</result>");
    }
    Ok(())
}

/// Expand a glob pattern into a list of file paths
pub fn expand_glob(root: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    use std::fs;

    // If it's a simple path (no glob chars), just return it
    if !pattern.contains('*') && !pattern.contains('?') {
        let path = if Path::new(pattern).is_absolute() {
            PathBuf::from(pattern)
        } else {
            root.join(pattern)
        };
        return Ok(vec![path]);
    }

    let mut results = Vec::new();
    let glob_pattern = if Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        root.join(pattern).to_string_lossy().to_string()
    };

    let parts: Vec<&str> = glob_pattern.split("**").collect();

    fn walk_dir(dir: &Path, results: &mut Vec<PathBuf>, pattern: &str) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    walk_dir(&path, results, pattern);
                } else if matches_pattern(&path, pattern) {
                    results.push(path);
                }
            }
        }
    }

    fn matches_pattern(path: &Path, pattern: &str) -> bool {
        let path_str = path.to_string_lossy();

        // Simple wildcard matching
        if pattern.contains("**") {
            // Handle **/*.rs style patterns
            if let Some(ext) = pattern.strip_prefix("**/") {
                if ext.starts_with("*.") {
                    let ext = ext.strip_prefix("*.").unwrap();
                    return path.extension().map(|e| e == ext).unwrap_or(false);
                }
                return path_str.ends_with(ext);
            }
        }

        if pattern.contains('*') {
            // Handle *.rs style patterns
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return (prefix.is_empty() || path_str.starts_with(prefix))
                    && (suffix.is_empty() || path_str.ends_with(suffix));
            }
        }

        path_str.contains(pattern)
    }

    if parts.len() > 1 {
        // Has ** in pattern
        let base = if parts[0].is_empty() {
            root.to_path_buf()
        } else {
            PathBuf::from(parts[0].trim_end_matches('/'))
        };
        walk_dir(&base, &mut results, &glob_pattern);
    } else {
        // Simple glob
        let parent = Path::new(&glob_pattern).parent().unwrap_or(root);
        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if matches_pattern(&path, &glob_pattern) {
                    results.push(path);
                }
            }
        }
    }

    Ok(results)
}

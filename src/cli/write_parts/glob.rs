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

#[cfg(test)]
mod tests {
    use super::{line_change_summary, lock_path, resolve_path, ChangeSummary};
    use std::path::Path;

    #[test]
    fn regression_cli_lock_path_is_repo_relative() {
        let root = Path::new("/repo");
        let path = Path::new("/repo/src/auth.py");

        assert_eq!(lock_path(root, path, "src/auth.py"), "src/auth.py");
    }

    #[test]
    fn regression_cli_resolves_relative_paths_under_root() {
        let root = Path::new("/repo");

        assert_eq!(
            resolve_path(root, "src/auth.py"),
            Path::new("/repo/src/auth.py")
        );
    }

    #[test]
    fn regression_symbol_lock_name_is_lockd_safe_and_stable() {
        let first = super::symbol_lock_name("src/auth.py", "Auth.login!");
        let second = super::symbol_lock_name("src/auth.py", "Auth.login!");

        assert_eq!(first, second);
        assert!(first.starts_with("sym:"));
        assert!(first
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == ':'));
    }

    #[test]
    fn regression_line_change_summary_detects_middle_replace() {
        assert_eq!(
            line_change_summary(Some("a\nb\nc\n"), "a\nB\nc\n"),
            ChangeSummary {
                start_line: 2,
                old_end_line: 2,
                new_end_line: 2,
                old_changed_lines: 1,
                new_changed_lines: 1,
            }
        );
    }

    #[test]
    fn regression_line_change_summary_detects_insert() {
        assert_eq!(
            line_change_summary(Some("a\nc\n"), "a\nb\nc\n"),
            ChangeSummary {
                start_line: 2,
                old_end_line: 1,
                new_end_line: 2,
                old_changed_lines: 0,
                new_changed_lines: 1,
            }
        );
    }
}

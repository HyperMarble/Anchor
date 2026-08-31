fn execution_summary(root: &Path, events: &[events::ExecutionEvent]) -> Result<events::EventSummary> {
    Ok(events::EventSummary::from_events(events)
        .with_unrecorded_repo_changes(git_changed_paths(root)?))
}

fn git_changed_paths(root: &Path) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .arg("--untracked-files=all")
        .output()
        .with_context(|| {
            "failed to launch Git; install Git for Windows and ensure git.exe is on PATH"
        })?;

    if !output.status.success() {
        // Anchor can still report a clean execution session in a directory
        // that has not been initialized as a Git repository. A missing Git
        // executable is different and is reported by the launch error above.
        return Ok(Vec::new());
    }

    let mut paths = std::collections::BTreeSet::new();
    for path in git_status_paths(&output.stdout) {
        if is_repo_audit_path(&path) {
            paths.insert(path);
        }
    }
    Ok(paths.into_iter().collect())
}

fn git_status_paths(output: &[u8]) -> Vec<String> {
    let records: Vec<&[u8]> = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect();
    let mut paths = Vec::new();
    let mut index = 0usize;

    while index < records.len() {
        let record = records[index];
        if record.len() < 4 || record[2] != b' ' {
            index += 1;
            continue;
        }

        let status_x = record[0];
        let status_y = record[1];
        let path = String::from_utf8_lossy(&record[3..]).replace('\\', "/");
        if !path.is_empty() {
            paths.push(path);
        }

        // In porcelain v1 -z output a rename/copy is encoded as
        // "XY destination\0source\0". The destination is the changed path;
        // consume the following source record so it is not mistaken for a
        // standalone status entry.
        if matches!(status_x, b'R' | b'C') || matches!(status_y, b'R' | b'C') {
            index += 1;
        }
        index += 1;
    }

    paths
}

fn is_repo_audit_path(path: &str) -> bool {
    let path = path.replace('\\', "/");
    if path.starts_with(".anchor/")
        || path.starts_with(".git/")
        || path.starts_with(".cache/")
        || path.starts_with(".mypy_cache/")
        || path.starts_with(".pytest_cache/")
        || path.starts_with(".ruff_cache/")
        || path.starts_with(".venv/")
        || path.contains("/__pycache__/")
        || path.ends_with(".pyc")
        || path.ends_with(".pyo")
    {
        return false;
    }
    true
}

#[cfg(test)]
mod repo_audit_tests {
    use super::git_status_paths;

    #[test]
    fn parses_nul_delimited_paths_with_spaces_and_unicode() {
        let output = b" M src/file with spaces.rs\0?? src/na\xC3\xAFve.rs\0";
        assert_eq!(
            git_status_paths(output),
            vec!["src/file with spaces.rs", "src/na\u{00ef}ve.rs"]
        );
    }

    #[test]
    fn rename_uses_destination_and_skips_source_record() {
        let output = b"R  src/new name.rs\0src/old name.rs\0";
        assert_eq!(git_status_paths(output), vec!["src/new name.rs"]);
    }
}

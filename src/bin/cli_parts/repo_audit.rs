fn execution_summary(root: &Path, events: &[events::ExecutionEvent]) -> events::EventSummary {
    events::EventSummary::from_events(events).with_unrecorded_repo_changes(git_changed_paths(root))
}

fn git_changed_paths(root: &Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain")
        .arg("--untracked-files=all")
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let mut paths = std::collections::BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(path) = git_status_path(line).filter(|path| is_repo_audit_path(path)) {
            paths.insert(path);
        }
    }
    paths.into_iter().collect()
}

fn git_status_path(line: &str) -> Option<String> {
    if line.len() < 4 {
        return None;
    }
    let path = if line.starts_with("R ") || line.starts_with("RM") || line.starts_with("RD") {
        line.get(3..)?
            .rsplit_once(" -> ")
            .map(|(_, to)| to)
            .unwrap_or_else(|| line.get(3..).unwrap_or_default())
    } else {
        line.get(3..)?
    };
    let path = path.trim().trim_matches('"').replace('\\', "/");
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
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

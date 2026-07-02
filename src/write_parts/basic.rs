/// Read a file, returning a FileNotFound error if missing.
fn read_file(path: &Path) -> Result<String, WriteError> {
    fs::read_to_string(path).map_err(|_| WriteError::FileNotFound(path.to_path_buf()))
}

/// Create a new file with the given content.
pub fn create_file(path: &Path, content: &str) -> Result<WriteResult, WriteError> {
    record_ungoverned_write(path, "create_file");
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
    record_ungoverned_write(path, "insert_after");
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

/// Replace all occurrences of a pattern with new content.
pub fn replace_all(
    path: &Path,
    old_pattern: &str,
    new_content: &str,
) -> Result<WriteResult, WriteError> {
    record_ungoverned_write(path, "replace_all");
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

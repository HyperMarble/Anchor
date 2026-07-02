/// Replace a line range in a file. Line numbers are 1-indexed.
/// This is a range-aware write: no string matching, just line numbers.
pub fn replace_range(
    path: &Path,
    start_line: usize,
    end_line: usize,
    new_content: &str,
) -> Result<WriteResult, WriteError> {
    record_ungoverned_write(path, "replace_range");
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

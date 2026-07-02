fn enclosing_text_block(lines: &[&str], hit: usize) -> Option<(usize, usize)> {
    let mut candidates = Vec::new();
    if let Some(range) = brace_enclosing_block(lines, hit) {
        candidates.push(range);
    }
    if let Some(range) = indentation_enclosing_block(lines, hit) {
        candidates.push(range);
    }
    candidates
        .into_iter()
        .min_by_key(|(start, end)| end.saturating_sub(*start))
}

fn brace_enclosing_block(lines: &[&str], hit: usize) -> Option<(usize, usize)> {
    let mut stack: Vec<(usize, usize, char)> = Vec::new();
    let mut best = None;
    for (line_idx, line) in lines.iter().enumerate() {
        let mut in_string = false;
        let mut quote = '\0';
        let mut escaped = false;
        for ch in line.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == quote {
                    in_string = false;
                }
                continue;
            }
            if ch == '"' || ch == '\'' {
                in_string = true;
                quote = ch;
            } else if matches!(ch, '{' | '(' | '[') {
                stack.push((line_idx, stack.len(), ch));
            } else if matches!(ch, '}' | ')' | ']') {
                let Some((open_line, depth, open)) = stack.pop() else {
                    continue;
                };
                if delimiter_matches(open, ch) {
                    if open_line <= hit && hit <= line_idx {
                        let candidate = (open_line, line_idx, depth);
                        if best
                            .map(|(_, _, best_depth)| depth > best_depth)
                            .unwrap_or(true)
                        {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }
    }
    best.map(|(start, end, _)| (start, end))
}

fn delimiter_matches(open: char, close: char) -> bool {
    matches!((open, close), ('{', '}') | ('(', ')') | ('[', ']'))
}

fn indentation_enclosing_block(lines: &[&str], hit: usize) -> Option<(usize, usize)> {
    if hit >= lines.len() {
        return None;
    }
    let hit_indent = line_indent(lines[hit]);
    let mut start = None;
    for idx in (0..=hit).rev() {
        let line = lines[idx];
        if line.trim().is_empty() {
            continue;
        }
        let indent = line_indent(line);
        let is_header = trimmed_code(line).ends_with(':');
        if is_header && (idx == hit || indent < hit_indent || start.is_none()) {
            start = Some((idx, indent));
            if idx == hit || indent < hit_indent {
                break;
            }
        }
    }
    let (start_idx, start_indent) = start?;
    let mut end = lines.len().saturating_sub(1);
    for (idx, line) in lines.iter().enumerate().skip(start_idx + 1) {
        if line.trim().is_empty() {
            continue;
        }
        if line_indent(line) <= start_indent {
            end = idx.saturating_sub(1);
            break;
        }
    }
    Some((start_idx, end.max(start_idx)))
}

fn line_indent(line: &str) -> usize {
    line.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum()
}

fn trimmed_code(line: &str) -> &str {
    line.split('#').next().unwrap_or(line).trim_end()
}

//
//  slice.rs
//  Anchor
//
//  Created by hak (tharun)
//

/// Result of slicing a symbol's code.
pub struct SliceResult {
    /// The sliced (or full) code string with line numbers
    pub code: String,
    /// Total lines in the original code
    pub total_lines: usize,
    /// Lines shown after slicing
    pub shown_lines: usize,
    /// Number of dependency call sites
    pub call_count: usize,
    /// Whether slicing was actually applied
    pub was_sliced: bool,
}

/// Slice a symbol's code to show only graph-relevant lines.
///
/// Keeps:
/// - First line (function signature)
/// - Last line (closing brace)
/// - Lines containing calls to graph dependencies (call_lines)
/// - 1 line of context above each call line (for if/assignment)
/// - Return statements
///
/// `call_lines` are absolute line numbers (1-indexed).
/// `line_start` is the symbol's starting line in the file (1-indexed).
pub fn slice_code(code: &str, call_lines: &[usize], line_start: usize) -> SliceResult {
    let lines: Vec<&str> = code.lines().collect();

    if lines.len() <= 10 || call_lines.is_empty() {
        // Short code or no calls — return full code, no slicing needed
        return SliceResult {
            code: code.to_string(),
            total_lines: lines.len(),
            shown_lines: lines.len(),
            call_count: call_lines.len(),
            was_sliced: false,
        };
    }

    let mut keep: Vec<bool> = vec![false; lines.len()];

    // Always keep first line (signature) and last line (closing brace)
    keep[0] = true;
    if lines.len() > 1 {
        keep[lines.len() - 1] = true;
    }

    // Keep lines with calls + 1 line of context above
    for &abs_line in call_lines {
        // Convert absolute line number to relative index within this symbol
        if abs_line >= line_start {
            let rel = abs_line - line_start;
            if rel < lines.len() {
                keep[rel] = true;
                // 1 line above for context (if/let/assignment)
                if rel > 0 {
                    keep[rel - 1] = true;
                }
                // 1 line below for context (closing brace of if, error handling)
                if rel + 1 < lines.len() {
                    keep[rel + 1] = true;
                }
            }
        }
    }

    // Also keep return statements
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("return ") || trimmed.starts_with("return;")
            || trimmed.starts_with("Ok(") || trimmed.starts_with("Err(")
            || trimmed.starts_with("raise ") || trimmed.starts_with("throw ")
        {
            keep[i] = true;
        }
    }

    let shown_lines = keep.iter().filter(|&&k| k).count();

    // Build output with line numbers, collapsing skipped sections
    let mut result = String::new();
    let mut in_gap = false;

    for (i, line) in lines.iter().enumerate() {
        if keep[i] {
            if in_gap {
                result.push_str("    ...\n");
                in_gap = false;
            }
            let abs_line_num = line_start + i;
            result.push_str(&format!("{:>4}: {}\n", abs_line_num, line));
        } else {
            in_gap = true;
        }
    }

    SliceResult {
        code: result,
        total_lines: lines.len(),
        shown_lines,
        call_count: call_lines.len(),
        was_sliced: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_short_code() {
        let code = "fn main() {\n    println!(\"hello\");\n}";
        let result = slice_code(code, &[2], 1);
        // Short code (3 lines) — returns full, no slicing
        assert!(!result.was_sliced);
        assert_eq!(result.code, code.to_string());
        assert_eq!(result.total_lines, 3);
        assert_eq!(result.shown_lines, 3);
    }

    #[test]
    fn test_slice_long_function() {
        let code = r#"pub fn login(user: &str, pw: &str) -> bool {
    let logger = setup_logger();
    logger.info("attempt");
    logger.debug("checking");
    logger.trace("details");
    logger.trace("more");
    logger.trace("stuff");
    logger.trace("padding");
    logger.trace("noise");
    logger.trace("filler");
    let valid = validate(user);
    if !valid {
        return false;
    }
    let ok = check_password(pw);
    println!("done");
    println!("more noise");
    println!("padding");
    valid && ok
}"#;
        // call_lines: validate at line 11, check_password at line 15 (absolute)
        let result = slice_code(code, &[11, 15], 1);

        assert!(result.was_sliced);
        assert!(result.shown_lines < result.total_lines);
        assert_eq!(result.call_count, 2);
        // Should contain signature, validate call, check_password call, return
        assert!(result.code.contains("pub fn login"));
        assert!(result.code.contains("validate(user)"));
        assert!(result.code.contains("check_password(pw)"));
        assert!(result.code.contains("..."));
        // Should NOT contain all the logger noise
        assert!(!result.code.contains("logger.trace(\"stuff\")"));
    }

    #[test]
    fn test_slice_no_calls() {
        let code = "fn simple() {\n    let x = 1;\n    let y = 2;\n    x + y\n}";
        let result = slice_code(code, &[], 1);
        // No calls — return full code
        assert!(!result.was_sliced);
        assert_eq!(result.code, code.to_string());
    }

    #[test]
    fn test_slice_preserves_returns() {
        let code = r#"fn process(input: &str) -> Result<String> {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
    let f = 6;
    let g = 7;
    let h = 8;
    let i = 9;
    let result = transform(input);
    Ok(result)
}"#;
        let result = slice_code(code, &[11], 1);
        assert!(result.was_sliced);
        assert!(result.code.contains("transform(input)"));
        assert!(result.code.contains("Ok(result)"));
    }

    #[test]
    fn test_slice_metadata() {
        let mut code = "fn big() {\n".to_string();
        for i in 1..=20 {
            code.push_str(&format!("    let x{} = {};\n", i, i));
        }
        code.push_str("    foo();\n}");
        let result = slice_code(&code, &[22], 1); // foo() at line 22
        assert!(result.was_sliced);
        assert!(result.shown_lines < result.total_lines);
        assert_eq!(result.call_count, 1);
        assert_eq!(result.total_lines, 23); // 1 sig + 20 lets + 1 call + 1 brace
    }
}

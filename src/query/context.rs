//
//  context.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::fs;
use std::path::Path;

use crate::graph::{CodeGraph, DependencyInfo, SearchResult};

use super::types::{ContextResponse, Edit, Reference, Signature, Symbol};

/// Get context for a symbol based on intent.
///
/// Intents:
/// - `explore` (default): Symbol + what it uses + what uses it
/// - `change`: Symbol + dependents + suggested edits + tests to update
/// - `create`: Symbol + similar patterns in codebase
pub fn get_context(graph: &CodeGraph, query: &str, intent: &str) -> ContextResponse {
    get_context_for_change(graph, query, intent, None)
}

/// Get context with optional new signature for change intent.
///
/// When changing a function signature, pass the new signature to get
/// suggested fixes for each call site.
///
/// Example:
/// ```ignore
/// get_context_for_change(
///     graph,
///     "validate",
///     "change",
///     Some("validate(input: &str, strict: bool) -> bool")
/// )
/// ```
pub fn get_context_for_change(
    graph: &CodeGraph,
    query: &str,
    intent: &str,
    new_signature: Option<&str>,
) -> ContextResponse {
    let mut response = ContextResponse {
        query: query.to_string(),
        intent: intent.to_string(),
        ..Default::default()
    };

    // Find the symbol first
    let results = graph.search(query, 5);
    if results.is_empty() {
        return response;
    }

    response.found = true;
    response.symbols = results.iter().map(Symbol::from_search_result).collect();

    match intent {
        "explore" => explore(graph, query, &results, &mut response),
        "change" => change(graph, query, &results, new_signature, &mut response),
        "create" => create(graph, query, &results, &mut response),
        _ => explore(graph, query, &results, &mut response), // default
    }

    response
}

/// Explore intent: understand what something is and how it connects.
fn explore(
    graph: &CodeGraph,
    query: &str,
    _results: &[SearchResult],
    response: &mut ContextResponse,
) {
    // What uses this symbol (dependents)
    let dependents = graph.dependents(query);
    response.used_by = dependents.iter().map(Reference::from_dep).collect();

    // What this symbol uses (dependencies)
    let dependencies = graph.dependencies(query);
    response.uses = dependencies.iter().map(Reference::from_dep).collect();
}

/// Change intent: what breaks if I modify this, and how to fix it.
fn change(
    graph: &CodeGraph,
    query: &str,
    _results: &[SearchResult],
    new_signature: Option<&str>,
    response: &mut ContextResponse,
) {
    // Get all dependents - these will need updates
    let dependents = graph.dependents(query);
    response.used_by = dependents.iter().map(Reference::from_dep).collect();

    // Parse old signature from the symbol's code
    let old_sig = graph
        .search(query, 1)
        .first()
        .and_then(|r| extract_signature_from_code(&r.code));

    // Parse new signature if provided
    let new_sig = new_signature.and_then(Signature::parse);

    // Calculate signature diff if both available
    let sig_diff = match (&old_sig, &new_sig) {
        (Some(old), Some(new)) => Some(old.diff(new)),
        _ => None,
    };

    // Build suggested edits with ACTUAL usage extraction
    for dep in &dependents {
        if let Some(edit) = build_edit(graph, query, dep, &new_sig, &sig_diff) {
            response.edits.push(edit);
        }
    }

    // Find related tests
    response.tests = find_tests(graph, query);
}

/// Extract function signature from code snippet.
fn extract_signature_from_code(code: &str) -> Option<Signature> {
    // Find the first line that looks like a function definition
    for line in code.lines() {
        let line = line.trim();
        // Rust: fn name(...) or pub fn name(...)
        if line.starts_with("fn ") || line.contains(" fn ") {
            // Extract from "fn" to the opening brace or end of params
            if let Some(fn_start) = line.find("fn ") {
                let rest = &line[fn_start..];
                // Find the end of signature (before { or just the line)
                let sig_end = rest.find('{').unwrap_or(rest.len());
                let sig_str = rest[..sig_end].trim();
                return Signature::parse(sig_str);
            }
        }
        // Python: def name(...):
        if line.starts_with("def ") {
            let sig_end = line.find(':').unwrap_or(line.len());
            let sig_str = &line[4..sig_end]; // skip "def "
            return Signature::parse(&format!("{})", sig_str.trim_end_matches(')')));
        }
        // JS/TS: function name(...) or name(...) =>
        if line.starts_with("function ") {
            let sig_end = line.find('{').unwrap_or(line.len());
            return Signature::parse(&line[9..sig_end]); // skip "function "
        }
    }
    None
}

/// Build an Edit with actual usage extracted from source.
fn build_edit(
    graph: &CodeGraph,
    target_symbol: &str,
    dep: &DependencyInfo,
    new_sig: &Option<Signature>,
    sig_diff: &Option<(Vec<super::types::Param>, Vec<super::types::Param>)>,
) -> Option<Edit> {
    // Get the caller's code snippet from the graph
    let caller_code = graph
        .search(&dep.symbol, 1)
        .first()
        .map(|r| r.code.clone())?;

    // Find lines in the caller that reference the target symbol
    let usages = find_usages_in_code(&caller_code, target_symbol);

    if usages.is_empty() {
        // Fallback: couldn't find specific usage, return the whole function
        return Some(Edit {
            file: dep.file.to_string_lossy().to_string(),
            line: dep.line,
            in_symbol: dep.symbol.clone(),
            usage: format!("{}(...)", target_symbol),
            line_content: caller_code.lines().next().unwrap_or("").to_string(),
            suggested: None,
            new_args: vec![],
            removed_args: vec![],
            context: vec![],
        });
    }

    // Get the first usage (most common case)
    let (line_offset, usage_expr, line_content) = &usages[0];
    let actual_line = dep.line + line_offset;

    // Get context from the file if possible
    let context = get_context_lines(&dep.file, actual_line, 2);

    // Generate suggested fix if we have signature diff
    let (suggested, new_args, removed_args) = match (new_sig, sig_diff) {
        (Some(new_sig), Some((added, removed))) => {
            let suggested = generate_suggested_call(usage_expr, new_sig, added);
            let new_args: Vec<String> = added
                .iter()
                .map(|p| format!("{}: {}", p.name, p.typ))
                .collect();
            let removed_args: Vec<String> = removed.iter().map(|p| p.name.clone()).collect();
            (Some(suggested), new_args, removed_args)
        }
        _ => (None, vec![], vec![]),
    };

    Some(Edit {
        file: dep.file.to_string_lossy().to_string(),
        line: actual_line,
        in_symbol: dep.symbol.clone(),
        usage: usage_expr.clone(),
        line_content: line_content.clone(),
        suggested,
        new_args,
        removed_args,
        context,
    })
}

/// Generate a suggested call with new parameters.
fn generate_suggested_call(
    current_usage: &str,
    new_sig: &Signature,
    added_params: &[super::types::Param],
) -> String {
    // Parse current call to get existing arguments
    let current_args = extract_call_args(current_usage);

    // Build new argument list
    let mut new_args = current_args.clone();

    // Add placeholders for new parameters
    for param in added_params {
        let placeholder = format!("<{}>", param.name);
        new_args.push(placeholder);
    }

    // Reconstruct the call
    format!("{}({})", new_sig.name, new_args.join(", "))
}

/// Extract arguments from a call expression.
fn extract_call_args(call: &str) -> Vec<String> {
    let Some(open_paren) = call.find('(') else {
        return vec![];
    };
    let Some(close_paren) = call.rfind(')') else {
        return vec![];
    };

    let args_str = &call[open_paren + 1..close_paren];
    if args_str.trim().is_empty() {
        return vec![];
    }

    // Simple split - doesn't handle nested parens in args perfectly
    // but works for most cases
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in args_str.chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }

    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }

    args
}

/// Find usages of a symbol in code. Returns (line_offset, usage_expr, full_line).
fn find_usages_in_code(code: &str, symbol: &str) -> Vec<(usize, String, String)> {
    let mut usages = Vec::new();

    for (line_idx, line) in code.lines().enumerate() {
        // Look for the symbol followed by ( - it's a call
        if let Some(start) = line.find(symbol) {
            let after_symbol = &line[start..];

            // Check if it's a function call (symbol followed by parenthesis)
            if after_symbol.len() > symbol.len() {
                let next_char = after_symbol.chars().nth(symbol.len());
                if next_char == Some('(') {
                    // Extract the full call expression: symbol(...)
                    if let Some(usage) = extract_call_expression(after_symbol) {
                        usages.push((line_idx, usage, line.trim().to_string()));
                    }
                }
            }
        }
    }

    usages
}

/// Extract a function call expression from code starting at the call.
/// "validate(input, config)" from "let x = validate(input, config);"
fn extract_call_expression(code: &str) -> Option<String> {
    let mut depth = 0;
    let mut end_idx = 0;
    let mut started = false;

    for (i, c) in code.char_indices() {
        match c {
            '(' => {
                depth += 1;
                started = true;
            }
            ')' => {
                depth -= 1;
                if started && depth == 0 {
                    end_idx = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    if end_idx > 0 {
        Some(code[..end_idx].to_string())
    } else {
        None
    }
}

/// Read context lines from a file around a specific line.
fn get_context_lines(file_path: &Path, line: usize, context_size: usize) -> Vec<String> {
    let Ok(content) = fs::read_to_string(file_path) else {
        return vec![];
    };

    let lines: Vec<&str> = content.lines().collect();
    if line == 0 || line > lines.len() {
        return vec![];
    }

    let start = line.saturating_sub(context_size + 1);
    let end = (line + context_size).min(lines.len());

    lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let line_num = start + i + 1;
            if line_num == line {
                format!(">{:4}| {}", line_num, l)
            } else {
                format!(" {:4}| {}", line_num, l)
            }
        })
        .collect()
}

/// Create intent: find similar patterns to follow.
fn create(
    graph: &CodeGraph,
    _query: &str,
    results: &[SearchResult],
    response: &mut ContextResponse,
) {
    if let Some(reference) = results.first() {
        response.patterns = find_similar(graph, reference);
    }
}

/// Find test functions related to a symbol.
fn find_tests(graph: &CodeGraph, symbol: &str) -> Vec<Symbol> {
    let mut tests = Vec::new();

    // Look for test functions that reference this symbol
    let test_results = graph.search("test", 50);
    for result in test_results {
        let name_lower = result.symbol.to_lowercase();
        if name_lower.contains("test") && result.code.contains(symbol) {
            tests.push(Symbol::from_search_result(&result));
            if tests.len() >= 5 {
                break;
            }
        }
    }

    // Also check dependents - tests call the function
    let deps = graph.dependents(symbol);
    for dep in deps {
        if dep.symbol.to_lowercase().contains("test") {
            if let Some(result) = graph.search(&dep.symbol, 1).first() {
                if !tests.iter().any(|t| t.name == dep.symbol) {
                    tests.push(Symbol::from_search_result(result));
                    if tests.len() >= 5 {
                        break;
                    }
                }
            }
        }
    }

    tests
}

/// Find similar symbols (same kind, nearby files).
fn find_similar(graph: &CodeGraph, reference: &SearchResult) -> Vec<Symbol> {
    let mut similar = Vec::new();

    // Get symbols of the same kind
    let all = graph.search("", 100);
    for result in all {
        if result.kind == reference.kind && result.symbol != reference.symbol {
            // Prefer same directory
            let same_dir = result.file.parent() == reference.file.parent();
            if same_dir || similar.len() < 3 {
                similar.push(Symbol::from_search_result(&result));
            }
            if similar.len() >= 5 {
                break;
            }
        }
    }

    similar
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::Signature;
    use crate::parser;
    use std::path::PathBuf;

    fn build_test_graph() -> CodeGraph {
        let source = r#"
pub fn process(input: &str) -> String {
    validate(input);
    transform(input)
}

fn validate(s: &str) -> bool {
    !s.is_empty()
}

fn transform(s: &str) -> String {
    s.to_uppercase()
}

#[test]
fn test_process() {
    assert_eq!(process("hi"), "HI");
}
"#;
        let path = PathBuf::from("src/lib.rs");
        let extraction = parser::extract_file(&path, source).unwrap();
        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);
        graph
    }

    #[test]
    fn test_explore_intent() {
        let graph = build_test_graph();
        let response = get_context(&graph, "validate", "explore");

        assert!(response.found);
        assert!(!response.symbols.is_empty());
        assert_eq!(response.intent, "explore");
    }

    #[test]
    fn test_change_intent() {
        let graph = build_test_graph();
        let response = get_context(&graph, "validate", "change");

        assert!(response.found);
        assert_eq!(response.intent, "change");
        // Should have edits for dependents
    }

    #[test]
    fn test_create_intent() {
        let graph = build_test_graph();
        let response = get_context(&graph, "validate", "create");

        assert!(response.found);
        assert_eq!(response.intent, "create");
        // Should find similar functions like transform
    }

    #[test]
    fn test_extract_call_expression() {
        assert_eq!(
            extract_call_expression("validate(input)"),
            Some("validate(input)".to_string())
        );
        assert_eq!(
            extract_call_expression("validate(a, b, c)"),
            Some("validate(a, b, c)".to_string())
        );
        assert_eq!(
            extract_call_expression("validate(nested(x))"),
            Some("validate(nested(x))".to_string())
        );
    }

    #[test]
    fn test_find_usages_in_code() {
        let code = r#"
fn process(input: &str) {
    let valid = validate(input);
    if valid {
        transform(input);
    }
}
"#;
        let usages = find_usages_in_code(code, "validate");
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].1, "validate(input)");

        let usages = find_usages_in_code(code, "transform");
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].1, "transform(input)");
    }

    #[test]
    fn test_signature_parsing() {
        // Rust style
        let sig = Signature::parse("fn validate(input: &str) -> bool").unwrap();
        assert_eq!(sig.name, "validate");
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0].name, "input");
        assert_eq!(sig.params[0].typ, "&str");
        assert_eq!(sig.return_type, Some("bool".to_string()));

        // Multiple params
        let sig = Signature::parse("validate(input: &str, strict: bool) -> bool").unwrap();
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[1].name, "strict");

        // No return type
        let sig = Signature::parse("process(data: Vec<u8>)").unwrap();
        assert_eq!(sig.return_type, None);
    }

    #[test]
    fn test_signature_diff() {
        let old = Signature::parse("validate(input: &str) -> bool").unwrap();
        let new = Signature::parse("validate(input: &str, strict: bool) -> bool").unwrap();

        let (added, removed) = old.diff(&new);
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].name, "strict");
        assert_eq!(removed.len(), 0);
    }

    #[test]
    fn test_signature_aware_change() {
        let graph = build_test_graph();

        // Without new_signature - just shows usage
        let response = get_context_for_change(&graph, "validate", "change", None);
        assert!(response.found);
        for edit in &response.edits {
            assert!(edit.suggested.is_none());
        }

        // With new_signature - shows suggested fix
        let response = get_context_for_change(
            &graph,
            "validate",
            "change",
            Some("validate(s: &str, strict: bool) -> bool"),
        );
        assert!(response.found);
        // Edits should have suggestions with the new parameter
        for edit in &response.edits {
            if edit.suggested.is_some() {
                assert!(edit.new_args.iter().any(|a| a.contains("strict")));
            }
        }
    }

    #[test]
    fn test_extract_call_args() {
        assert_eq!(extract_call_args("foo()"), Vec::<String>::new());
        assert_eq!(extract_call_args("foo(x)"), vec!["x"]);
        assert_eq!(extract_call_args("foo(x, y)"), vec!["x", "y"]);
        assert_eq!(extract_call_args("foo(a, b, c)"), vec!["a", "b", "c"]);
        // Nested calls
        assert_eq!(extract_call_args("foo(bar(x), y)"), vec!["bar(x)", "y"]);
    }
}

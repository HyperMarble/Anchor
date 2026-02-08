//! Read/Search operations: search, read, context
//!
//! All operations go through GraphQL queries internally.
//! This ensures consistent behavior between CLI and any future API.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::graph::CodeGraph;
use crate::graphql::{build_schema, execute};

/// Search for symbols by name or pattern.
///
/// Wraps GraphQL `symbol` query with optional regex pattern.
pub fn search(graph: &CodeGraph, query: &str, pattern: Option<&str>, limit: usize) -> Result<()> {
    let schema = build_schema(Arc::new(graph.clone()));

    // Build GraphQL query based on whether pattern is provided
    let gql_query = if let Some(pat) = pattern {
        // Use regex search
        format!(
            r#"{{ search(pattern: "{}", limit: {}) {{ name kind file line code }} }}"#,
            escape_graphql(pat),
            limit
        )
    } else {
        // Use symbol query with prefix matching
        format!(
            r#"{{ symbol(name: "{}") {{ name kind file line }} }}"#,
            escape_graphql(query)
        )
    };

    // Execute GraphQL query
    let result = tokio::runtime::Runtime::new()?.block_on(execute(&schema, &gql_query));

    // Parse and format output
    let json: serde_json::Value = serde_json::from_str(&result)?;

    if let Some(errors) = json.get("errors") {
        if let Some(arr) = errors.as_array() {
            if !arr.is_empty() {
                if let Some(msg) = arr[0].get("message") {
                    println!("Error: {}", msg.as_str().unwrap_or("unknown"));
                    return Ok(());
                }
            }
        }
    }

    let data = json.get("data");

    // Handle search results (from regex search)
    if let Some(symbols) = data.and_then(|d| d.get("search")).and_then(|s| s.as_array()) {
        if symbols.is_empty() {
            println!("No symbols match pattern '{}'", pattern.unwrap_or(query));
            return Ok(());
        }
        for sym in symbols.iter().take(limit) {
            print_symbol_compact(sym);
        }
        return Ok(());
    }

    // Handle symbol results (from name search)
    if let Some(symbols) = data.and_then(|d| d.get("symbol")).and_then(|s| s.as_array()) {
        if symbols.is_empty() {
            println!("No results for '{}'", query);
            return Ok(());
        }
        for sym in symbols.iter().take(limit) {
            print_symbol_compact(sym);
        }
        return Ok(());
    }

    println!("No results for '{}'", query);
    Ok(())
}

/// Read full context for a symbol.
///
/// Wraps GraphQL `symbol` query with callers/callees.
pub fn read(graph: &CodeGraph, symbol: &str) -> Result<()> {
    let schema = build_schema(Arc::new(graph.clone()));

    // GraphQL query: symbol with code, callers, callees
    let gql_query = format!(
        r#"{{ symbol(name: "{}", exact: true) {{ name kind file line code callers {{ name }} callees {{ name }} }} }}"#,
        escape_graphql(symbol)
    );

    let result = tokio::runtime::Runtime::new()?.block_on(execute(&schema, &gql_query));
    let json: serde_json::Value = serde_json::from_str(&result)?;

    if let Some(errors) = json.get("errors") {
        if let Some(arr) = errors.as_array() {
            if !arr.is_empty() {
                if let Some(msg) = arr[0].get("message") {
                    println!("Error: {}", msg.as_str().unwrap_or("unknown"));
                    return Ok(());
                }
            }
        }
    }

    let symbols = json
        .get("data")
        .and_then(|d| d.get("symbol"))
        .and_then(|s| s.as_array());

    let symbols = match symbols {
        Some(s) if !s.is_empty() => s,
        _ => {
            println!("Symbol '{}' not found", symbol);
            return Ok(());
        }
    };

    let sym = &symbols[0];

    // Header: symbol Kind file:line
    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);

    let file_name = Path::new(file)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| file.to_string());

    println!("{} {} {}:{}", name, kind, file_name, line);

    // Callers (who calls this) - unique names only
    if let Some(callers) = sym.get("callers").and_then(|c| c.as_array()) {
        let mut caller_names: Vec<&str> = callers
            .iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .filter(|n| !is_file_name(n))
            .collect();
        caller_names.sort();
        caller_names.dedup();
        if !caller_names.is_empty() {
            println!("> {}", caller_names.join(" "));
        }
    }

    // Callees (what this calls) - unique names only
    if let Some(callees) = sym.get("callees").and_then(|c| c.as_array()) {
        let mut callee_names: Vec<&str> = callees
            .iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .filter(|n| !is_file_name(n))
            .collect();
        callee_names.sort();
        callee_names.dedup();
        if !callee_names.is_empty() {
            println!("< {}", callee_names.join(" "));
        }
    }

    // Code
    if let Some(code) = sym.get("code").and_then(|c| c.as_str()) {
        println!("---");
        println!("{}", code);
    }

    Ok(())
}

/// Context: Search + Read combined.
///
/// Wraps GraphQL `symbol` query with code and relationships.
pub fn context(graph: &CodeGraph, query: &str, limit: usize) -> Result<()> {
    let schema = build_schema(Arc::new(graph.clone()));

    // GraphQL query: symbol search with code, callers, callees
    let gql_query = format!(
        r#"{{ symbol(name: "{}") {{ name kind file line code callers {{ name }} callees {{ name }} }} }}"#,
        escape_graphql(query)
    );

    let result = tokio::runtime::Runtime::new()?.block_on(execute(&schema, &gql_query));
    let json: serde_json::Value = serde_json::from_str(&result)?;

    if let Some(errors) = json.get("errors") {
        if let Some(arr) = errors.as_array() {
            if !arr.is_empty() {
                if let Some(msg) = arr[0].get("message") {
                    println!("Error: {}", msg.as_str().unwrap_or("unknown"));
                    return Ok(());
                }
            }
        }
    }

    let symbols = json
        .get("data")
        .and_then(|d| d.get("symbol"))
        .and_then(|s| s.as_array());

    let symbols = match symbols {
        Some(s) if !s.is_empty() => s,
        _ => {
            println!("No results for '{}'", query);
            return Ok(());
        }
    };

    for (i, sym) in symbols.iter().take(limit).enumerate() {
        if i > 0 {
            println!("\n===");
        }

        // Header: symbol Kind file:line
        let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
        let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);

        let file_name = Path::new(file)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| file.to_string());

        println!("{} {} {}:{}", name, kind, file_name, line);

        // Callers - unique names only
        if let Some(callers) = sym.get("callers").and_then(|c| c.as_array()) {
            let mut caller_names: Vec<&str> = callers
                .iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
                .filter(|n| !is_file_name(n))
                .collect();
            caller_names.sort();
            caller_names.dedup();
            if !caller_names.is_empty() {
                println!("> {}", caller_names.join(" "));
            }
        }

        // Callees - unique names only
        if let Some(callees) = sym.get("callees").and_then(|c| c.as_array()) {
            let mut callee_names: Vec<&str> = callees
                .iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
                .filter(|n| !is_file_name(n))
                .collect();
            callee_names.sort();
            callee_names.dedup();
            if !callee_names.is_empty() {
                println!("< {}", callee_names.join(" "));
            }
        }

        // Code
        if let Some(code) = sym.get("code").and_then(|c| c.as_str()) {
            println!("---");
            println!("{}", code);
        }
    }

    Ok(())
}

/// Build/rebuild the code graph
pub fn build(root: &Path, cache_path: &Path) -> Result<()> {
    println!("Building...");
    let graph = crate::graph::build_graph(root);
    std::fs::create_dir_all(cache_path.parent().unwrap())?;
    graph.save(cache_path)?;

    let stats = graph.stats();
    println!("files:{} symbols:{} edges:{}", stats.file_count, stats.symbol_count, stats.total_edges);
    Ok(())
}

/// Get graph stats via GraphQL
pub fn stats(graph: &CodeGraph) -> Result<()> {
    let schema = build_schema(Arc::new(graph.clone()));

    let gql_query = "{ stats { files symbols edges } }";
    let result = tokio::runtime::Runtime::new()?.block_on(execute(&schema, gql_query));
    let json: serde_json::Value = serde_json::from_str(&result)?;

    if let Some(stats) = json.get("data").and_then(|d| d.get("stats")) {
        let files = stats.get("files").and_then(|v| v.as_i64()).unwrap_or(0);
        let symbols = stats.get("symbols").and_then(|v| v.as_i64()).unwrap_or(0);
        let edges = stats.get("edges").and_then(|v| v.as_i64()).unwrap_or(0);
        println!("files:{} symbols:{} edges:{}", files, symbols, edges);
    }

    Ok(())
}

/// Print a symbol in compact format: name Kind file:line
fn print_symbol_compact(sym: &serde_json::Value) {
    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);

    let file_name = Path::new(file)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| file.to_string());

    println!("{} {} {}:{}", name, kind, file_name, line);
}

/// Check if a string looks like a file name
fn is_file_name(s: &str) -> bool {
    s.ends_with(".rs") || s.ends_with(".py") || s.ends_with(".js") || s.ends_with(".ts")
}

/// Escape special characters for GraphQL string
fn escape_graphql(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Show codebase overview - files grouped by directory with symbol counts
pub fn overview(graph: &CodeGraph) -> Result<()> {
    let stats = graph.stats();
    println!("files:{} symbols:{} edges:{}", stats.file_count, stats.symbol_count, stats.total_edges);
    println!();

    // Get all files and group by directory
    let mut dirs: std::collections::BTreeMap<String, Vec<(String, usize)>> = std::collections::BTreeMap::new();

    for file_path in graph.all_files() {
        let symbols = graph.symbols_in_file(&file_path);
        let dir = file_path.parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let file_name = file_path.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        dirs.entry(dir).or_default().push((file_name, symbols.len()));
    }

    for (dir, files) in &dirs {
        println!("{}/", dir);
        for (file, count) in files {
            println!("  {} ({})", file, count);
        }
    }

    Ok(())
}

/// List all indexed files as tree
pub fn files(graph: &CodeGraph) -> Result<()> {
    for file_path in graph.all_files() {
        println!("{}", file_path.display());
    }
    Ok(())
}

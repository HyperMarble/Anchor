//! Read/Search operations: search, read, context
//!
//! All operations go through GraphQL queries internally.
//! This ensures consistent behavior between CLI and any future API.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::graph::CodeGraph;
use crate::graphql::{build_schema, execute};

/// Search for symbols by name or pattern (supports multiple queries).
///
/// Wraps GraphQL `search` query with optional regex pattern.
pub fn search(graph: &CodeGraph, queries: &[String], pattern: Option<&str>, limit: usize) -> Result<()> {
    let schema = build_schema(Arc::new(graph.clone()));
    let rt = tokio::runtime::Runtime::new()?;

    for (i, query) in queries.iter().enumerate() {
        if i > 0 {
            println!();
        }

        // Build GraphQL query - always use regex search for flexibility
        let gql_query = if let Some(pat) = pattern {
            format!(
                r#"{{ search(pattern: "{}", limit: {}) {{ name kind file line code }} }}"#,
                escape_graphql(pat),
                limit
            )
        } else {
            // Auto-convert query to regex: "send request" → ".*send.*request.*"
            let words: Vec<&str> = query.split_whitespace().collect();
            let regex_pat = if words.len() > 1 {
                format!(".*{}.*", words.join(".*")).to_lowercase()
            } else {
                format!(".*{}.*", query).to_lowercase()
            };
            format!(
                r#"{{ search(pattern: "{}", limit: {}) {{ name kind file line code }} }}"#,
                escape_graphql(&regex_pat),
                limit
            )
        };

        let result = rt.block_on(execute(&schema, &gql_query));
        let json: serde_json::Value = serde_json::from_str(&result)?;

        if let Some(errors) = json.get("errors") {
            if let Some(arr) = errors.as_array() {
                if !arr.is_empty() {
                    if let Some(msg) = arr[0].get("message") {
                        println!("Error: {}", msg.as_str().unwrap_or("unknown"));
                        continue;
                    }
                }
            }
        }

        let data = json.get("data");

        if let Some(symbols) = data.and_then(|d| d.get("search")).and_then(|s| s.as_array()) {
            if symbols.is_empty() {
                println!("No symbols match '{}'", query);
                continue;
            }
            for sym in symbols.iter().take(limit) {
                print_symbol_compact(sym);
            }
        }
    }

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

/// Context: Search + Read combined (supports multiple queries).
///
/// Wraps GraphQL `symbol` query with code and relationships.
pub fn context(graph: &CodeGraph, queries: &[String], limit: usize) -> Result<()> {
    let schema = build_schema(Arc::new(graph.clone()));
    let rt = tokio::runtime::Runtime::new()?;
    let mut first = true;

    for query in queries {
        let gql_query = format!(
            r#"{{ symbol(name: "{}") {{ name kind file line code callers {{ name }} callees {{ name }} }} }}"#,
            escape_graphql(query)
        );

        let result = rt.block_on(execute(&schema, &gql_query));
        let json: serde_json::Value = serde_json::from_str(&result)?;

        if let Some(errors) = json.get("errors") {
            if let Some(arr) = errors.as_array() {
                if !arr.is_empty() {
                    if let Some(msg) = arr[0].get("message") {
                        println!("Error: {}", msg.as_str().unwrap_or("unknown"));
                        continue;
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
                continue;
            }
        };

        for sym in symbols.iter().take(limit) {
            if !first {
                println!("\n===");
            }
            first = false;

            let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
            let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);

            let file_name = Path::new(file)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| file.to_string());

            println!("{} {} {}:{}", name, kind, file_name, line);

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

            if let Some(code) = sym.get("code").and_then(|c| c.as_str()) {
                println!("---");
                println!("{}", code);
            }
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

/// Show codebase map - compact view for AI agents
///
/// Format:
/// module(symbols) module(symbols) ...
/// ENTRY: symbols with no callers
/// TOP: most connected symbols
pub fn map(graph: &CodeGraph, scope: Option<&str>) -> Result<()> {
    use std::collections::{BTreeMap, HashSet};

    // Collect all symbols grouped by directory (module)
    let mut modules: BTreeMap<String, Vec<(String, String, usize, usize)>> = BTreeMap::new();
    let mut all_symbols: Vec<(String, String, usize, usize, String)> = Vec::new();

    for file_path in graph.all_files() {
        let dir = file_path.parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        // If scope is specified, filter to that module
        if let Some(s) = scope {
            if !dir.contains(s) && !file_path.to_string_lossy().contains(s) {
                continue;
            }
        }

        for symbol in graph.symbols_in_file(&file_path) {
            // Skip imports and files
            if matches!(symbol.kind, crate::graph::types::NodeKind::Import | crate::graph::types::NodeKind::File) {
                continue;
            }

            let callers = graph.dependents(&symbol.name).len();
            let callees = graph.dependencies(&symbol.name).len();
            let short_module = dir.split('/').last().unwrap_or(&dir).to_string();

            modules.entry(dir.clone())
                .or_default()
                .push((symbol.name.clone(), symbol.kind.to_string(), callers, callees));

            all_symbols.push((symbol.name.clone(), symbol.kind.to_string(), callers, callees, short_module));
        }
    }

    if modules.is_empty() {
        println!("No symbols found");
        return Ok(());
    }

    // If scope specified, show detailed view of that module
    if scope.is_some() {
        for (dir, symbols) in &modules {
            println!("@{}", dir);
            for (name, kind, callers, callees) in symbols {
                let mut parts = Vec::new();
                if *callees > 0 {
                    let deps: Vec<String> = graph.dependencies(name)
                        .iter()
                        .take(5)
                        .map(|d| d.symbol.clone())
                        .collect();
                    if !deps.is_empty() {
                        parts.push(format!(">{}", deps.join(",")));
                    }
                }
                if *callers > 0 {
                    let callers_list: Vec<String> = graph.dependents(name)
                        .iter()
                        .take(5)
                        .map(|d| d.symbol.clone())
                        .collect();
                    if !callers_list.is_empty() {
                        parts.push(format!("<{}", callers_list.join(",")));
                    }
                }
                if parts.is_empty() {
                    println!("  {}.{}", name, short_kind(kind));
                } else {
                    println!("  {}.{} {}", name, short_kind(kind), parts.join(" "));
                }
            }
        }
        return Ok(());
    }

    // Top level view: modules with counts
    let module_line: Vec<String> = modules.iter()
        .map(|(dir, symbols)| {
            let short_dir = dir.split('/').last().unwrap_or(dir);
            format!("{}({}s)", short_dir, symbols.len())
        })
        .collect();
    println!("{}", module_line.join(" "));

    // Entry points: functions/methods with 0 callers AND have callees (actually do something)
    let entries: Vec<String> = all_symbols.iter()
        .filter(|(name, kind, callers, callees, _)| {
            *callers == 0 && *callees > 0 &&
            (kind == "function" || kind == "method") &&
            !name.starts_with("test_") && name != "new"
        })
        .map(|(name, _, _, _, module)| format!("{}:{}", module, name))
        .take(10)
        .collect();

    if !entries.is_empty() {
        println!("ENTRY: {}", entries.join(" "));
    }

    // Top connected: symbols with most relationships (deduplicated by name)
    let mut by_connections = all_symbols.clone();
    by_connections.sort_by(|a, b| (b.2 + b.3).cmp(&(a.2 + a.3)));

    let mut seen: HashSet<String> = HashSet::new();
    let mut top: Vec<String> = Vec::new();

    for (name, kind, callers, callees, module) in by_connections.iter() {
        if kind == "import" || name == "new" {
            continue;
        }
        if seen.insert(name.clone()) {
            top.push(format!("{}:{}({})", module, name, callers + callees));
            if top.len() >= 10 {
                break;
            }
        }
    }

    if !top.is_empty() {
        println!("TOP: {}", top.join(" "));
    }

    Ok(())
}

/// Short kind abbreviation
fn short_kind(kind: &str) -> &str {
    match kind {
        "function" => "fn",
        "method" => "m",
        "struct" => "st",
        "class" => "cl",
        "trait" => "tr",
        "interface" => "if",
        "enum" => "en",
        "constant" => "c",
        "module" => "mod",
        "type" => "ty",
        "variable" => "v",
        "impl" => "impl",
        _ => kind,
    }
}

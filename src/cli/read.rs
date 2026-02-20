//
//  read.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::graph::CodeGraph;
use crate::graphql::{build_schema, execute};

/// Search for symbols by name or pattern (supports multiple queries).
///
/// Wraps GraphQL `search` query with optional regex pattern.
pub fn search(
    graph: &CodeGraph,
    queries: &[String],
    pattern: Option<&str>,
    limit: usize,
) -> Result<()> {
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
                        println!("<error>{}</error>", msg.as_str().unwrap_or("unknown"));
                        continue;
                    }
                }
            }
        }

        let data = json.get("data");

        if let Some(symbols) = data
            .and_then(|d| d.get("search"))
            .and_then(|s| s.as_array())
        {
            if symbols.is_empty() {
                println!("<results query=\"{}\" count=\"0\"/>", query);
                continue;
            }
            println!(
                "<results query=\"{}\" count=\"{}\">",
                query,
                symbols.len().min(limit)
            );
            for sym in symbols.iter().take(limit) {
                print_symbol_structured(sym);
            }
            println!("</results>");
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
                    println!("<error>{}</error>", msg.as_str().unwrap_or("unknown"));
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
            println!("<error>symbol '{}' not found</error>", symbol);
            return Ok(());
        }
    };

    let sym = &symbols[0];

    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);

    let mut caller_names: Vec<&str> = Vec::new();
    let mut callee_names: Vec<&str> = Vec::new();

    if let Some(callers) = sym.get("callers").and_then(|c| c.as_array()) {
        caller_names = callers
            .iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .filter(|n| !is_file_name(n))
            .collect();
        caller_names.sort();
        caller_names.dedup();
    }

    if let Some(callees) = sym.get("callees").and_then(|c| c.as_array()) {
        callee_names = callees
            .iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .filter(|n| !is_file_name(n))
            .collect();
        callee_names.sort();
        callee_names.dedup();
    }

    let code = sym.get("code").and_then(|c| c.as_str()).unwrap_or("");
    println!("<symbol>");
    println!("<name>{}</name>", name);
    println!("<kind>{}</kind>", kind);
    println!("<file>{}</file>", file);
    println!("<line>{}</line>", line);
    if !caller_names.is_empty() {
        println!("<callers>{}</callers>", caller_names.join(" "));
    }
    if !callee_names.is_empty() {
        println!("<callees>{}</callees>", callee_names.join(" "));
    }
    println!("<code>");
    println!("{}", code);
    println!("</code>");
    println!("</symbol>");

    Ok(())
}

/// Context: Search + Read combined (supports multiple queries).
///
/// Wraps GraphQL `symbol` query with code and relationships.
pub fn context(graph: &CodeGraph, queries: &[String], limit: usize, full: bool) -> Result<()> {
    let schema = build_schema(Arc::new(graph.clone()));
    let rt = tokio::runtime::Runtime::new()?;

    for query in queries {
        let gql_query = format!(
            r#"{{ symbol(name: "{}") {{ name kind file line code(full: {}) callers {{ name }} callees {{ name }} }} }}"#,
            escape_graphql(query),
            full,
        );

        let result = rt.block_on(execute(&schema, &gql_query));
        let json: serde_json::Value = serde_json::from_str(&result)?;

        if let Some(errors) = json.get("errors") {
            if let Some(arr) = errors.as_array() {
                if !arr.is_empty() {
                    if let Some(msg) = arr[0].get("message") {
                        println!("<error>{}</error>", msg.as_str().unwrap_or("unknown"));
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
                println!("<results query=\"{}\" count=\"0\"/>", query);
                continue;
            }
        };

        println!(
            "<results query=\"{}\" count=\"{}\">",
            query,
            symbols.len().min(limit)
        );

        for sym in symbols.iter().take(limit) {
            let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
            let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);

            let mut caller_names: Vec<&str> = Vec::new();
            let mut callee_names: Vec<&str> = Vec::new();

            if let Some(callers) = sym.get("callers").and_then(|c| c.as_array()) {
                caller_names = callers
                    .iter()
                    .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
                    .filter(|n| !is_file_name(n))
                    .collect();
                caller_names.sort();
                caller_names.dedup();
            }

            if let Some(callees) = sym.get("callees").and_then(|c| c.as_array()) {
                callee_names = callees
                    .iter()
                    .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
                    .filter(|n| !is_file_name(n))
                    .collect();
                callee_names.sort();
                callee_names.dedup();
            }

            let code = sym.get("code").and_then(|c| c.as_str()).unwrap_or("");
            println!("<symbol>");
            println!("<name>{}</name>", name);
            println!("<kind>{}</kind>", kind);
            println!("<file>{}</file>", file);
            println!("<line>{}</line>", line);
            if !caller_names.is_empty() {
                println!("<callers>{}</callers>", caller_names.join(" "));
            }
            if !callee_names.is_empty() {
                println!("<callees>{}</callees>", callee_names.join(" "));
            }
            println!("<code>");
            println!("{}", code);
            println!("</code>");
            println!("</symbol>");
        }

        println!("</results>");
    }

    Ok(())
}

/// Build/rebuild the code graph
pub fn build(root: &Path, cache_path: &Path) -> Result<()> {
    let graph = crate::graph::build_graph(root);
    std::fs::create_dir_all(cache_path.parent().unwrap())?;
    graph.save(cache_path)?;

    let stats = graph.stats();
    println!("<build>");
    println!("<files>{}</files>", stats.file_count);
    println!("<symbols>{}</symbols>", stats.symbol_count);
    println!("<edges>{}</edges>", stats.total_edges);
    println!("</build>");
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
        println!("<stats>");
        println!("<files>{}</files>", files);
        println!("<symbols>{}</symbols>", symbols);
        println!("<edges>{}</edges>", edges);
        println!("</stats>");
    }

    Ok(())
}

/// Print a symbol in structured format for AI
fn print_symbol_structured(sym: &serde_json::Value) {
    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);
    let code = sym.get("code").and_then(|v| v.as_str()).unwrap_or("");

    println!("<symbol>");
    println!("<name>{}</name>", name);
    println!("<kind>{}</kind>", kind);
    println!("<file>{}</file>", file);
    println!("<line>{}</line>", line);
    if !code.is_empty() {
        println!("<code>");
        for line in code.lines() {
            println!("{}", line);
        }
        println!("</code>");
    }
    println!("</symbol>");
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
    println!("<overview>");
    println!("<files>{}</files>", stats.file_count);
    println!("<symbols>{}</symbols>", stats.symbol_count);
    println!("<edges>{}</edges>", stats.total_edges);

    // Get all files and group by directory
    let mut dirs: std::collections::BTreeMap<String, Vec<(String, usize)>> =
        std::collections::BTreeMap::new();

    for file_path in graph.all_files() {
        let symbols = graph.symbols_in_file(&file_path);
        let dir = file_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let file_name = file_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        dirs.entry(dir)
            .or_default()
            .push((file_name, symbols.len()));
    }

    println!("<directories>");
    for (dir, files) in &dirs {
        println!("<dir path=\"{}\">", dir);
        for (file, count) in files {
            println!("<file name=\"{}\" symbols=\"{}\"/>", file, count);
        }
        println!("</dir>");
    }
    println!("</directories>");
    println!("</overview>");

    Ok(())
}

/// List all indexed files as tree
pub fn files(graph: &CodeGraph) -> Result<()> {
    let all_files = graph.all_files();
    println!("<files count=\"{}\">", all_files.len());
    for file_path in all_files {
        println!("<file>{}</file>", file_path.display());
    }
    println!("</files>");
    Ok(())
}

/// Show codebase map - compact view for AI agents
pub fn map(graph: &CodeGraph, scope: Option<&str>) -> Result<()> {
    use std::collections::{BTreeMap, HashSet};

    // Collect all symbols grouped by directory (module)
    let mut modules: BTreeMap<String, Vec<(String, String, usize, usize)>> = BTreeMap::new();
    let mut all_symbols: Vec<(String, String, usize, usize, String)> = Vec::new();

    for file_path in graph.all_files() {
        let dir = file_path
            .parent()
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
            if matches!(
                symbol.kind,
                crate::graph::types::NodeKind::Import | crate::graph::types::NodeKind::File
            ) {
                continue;
            }

            let callers = graph.dependents(&symbol.name).len();
            let callees = graph.dependencies(&symbol.name).len();
            let short_module = dir.split('/').last().unwrap_or(&dir).to_string();

            modules.entry(dir.clone()).or_default().push((
                symbol.name.clone(),
                symbol.kind.to_string(),
                callers,
                callees,
            ));

            all_symbols.push((
                symbol.name.clone(),
                symbol.kind.to_string(),
                callers,
                callees,
                short_module,
            ));
        }
    }

    if modules.is_empty() {
        println!("<map/>");
        return Ok(());
    }

    println!("<map>");

    // Modules with counts
    println!("<modules>");
    for (dir, symbols) in &modules {
        let short_dir = dir.split('/').last().unwrap_or(dir);
        println!(
            "<module name=\"{}\" symbols=\"{}\"/>",
            short_dir,
            symbols.len()
        );
    }
    println!("</modules>");

    // Entry points: functions/methods with 0 callers AND have callees
    let entries: Vec<String> = all_symbols
        .iter()
        .filter(|(name, kind, callers, callees, _)| {
            *callers == 0
                && *callees > 0
                && (kind == "function" || kind == "method")
                && !name.starts_with("test_")
                && name != "new"
        })
        .map(|(name, _, _, _, module)| format!("{}:{}", module, name))
        .take(10)
        .collect();

    if !entries.is_empty() {
        println!("<entry>{}</entry>", entries.join(" "));
    }

    // Top connected: symbols with most relationships
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
        println!("<top>{}</top>", top.join(" "));
    }

    println!("</map>");

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

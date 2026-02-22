//
//  read.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::graph::CodeGraph;
use crate::graphql::{build_schema, execute};

// ── Shared Helpers ───────────────────────────────────────────────────────────

/// Escape special characters for GraphQL string.
fn escape_graphql(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Check if a string looks like a file name (filter from callers/callees).
fn is_file_name(s: &str) -> bool {
    s.ends_with(".rs") || s.ends_with(".py") || s.ends_with(".js") || s.ends_with(".ts")
}

/// Extract first GraphQL error message, if any.
fn get_graphql_error(json: &serde_json::Value) -> Option<String> {
    json.get("errors")?
        .as_array()?
        .first()?
        .get("message")?
        .as_str()
        .map(|s| s.to_string())
}

/// Extract sorted, deduped names from a callers/callees JSON array.
fn extract_relationship_names<'a>(sym: &'a serde_json::Value, field: &str) -> Vec<&'a str> {
    let mut names: Vec<&str> = sym
        .get(field)
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
                .filter(|n| !is_file_name(n))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names.dedup();
    names
}

/// Print a symbol in XML format for AI consumption.
fn print_symbol_xml(sym: &serde_json::Value, include_relationships: bool) {
    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);
    let code = sym.get("code").and_then(|c| c.as_str()).unwrap_or("");

    println!("<symbol>");
    println!("<name>{}</name>", name);
    println!("<kind>{}</kind>", kind);
    println!("<file>{}</file>", file);
    println!("<line>{}</line>", line);

    if include_relationships {
        let callers = extract_relationship_names(sym, "callers");
        let callees = extract_relationship_names(sym, "callees");
        if !callers.is_empty() {
            println!("<callers>{}</callers>", callers.join(" "));
        }
        if !callees.is_empty() {
            println!("<callees>{}</callees>", callees.join(" "));
        }
    }

    if !code.is_empty() {
        println!("<code>");
        println!("{}", code);
        println!("</code>");
    }
    println!("</symbol>");
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Search for symbols by name or pattern (supports multiple queries).
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

        let gql_query = if let Some(pat) = pattern {
            format!(
                r#"{{ search(pattern: "{}", limit: {}) {{ name kind file line code }} }}"#,
                escape_graphql(pat),
                limit
            )
        } else {
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

        if let Some(err) = get_graphql_error(&json) {
            println!("<error>{}</error>", err);
            continue;
        }

        let symbols = json
            .get("data")
            .and_then(|d| d.get("search"))
            .and_then(|s| s.as_array());

        match symbols {
            Some(s) if !s.is_empty() => {
                println!(
                    "<results query=\"{}\" count=\"{}\">",
                    query,
                    s.len().min(limit)
                );
                for sym in s.iter().take(limit) {
                    print_symbol_xml(sym, false);
                }
                println!("</results>");
            }
            _ => println!("<results query=\"{}\" count=\"0\"/>", query),
        }
    }

    Ok(())
}

/// Read full context for a single symbol.
pub fn read(graph: &CodeGraph, symbol: &str) -> Result<()> {
    let schema = build_schema(Arc::new(graph.clone()));
    let gql_query = format!(
        r#"{{ symbol(name: "{}", exact: true) {{ name kind file line code callers {{ name }} callees {{ name }} }} }}"#,
        escape_graphql(symbol)
    );

    let result = tokio::runtime::Runtime::new()?.block_on(execute(&schema, &gql_query));
    let json: serde_json::Value = serde_json::from_str(&result)?;

    if let Some(err) = get_graphql_error(&json) {
        println!("<error>{}</error>", err);
        return Ok(());
    }

    let symbols = json
        .get("data")
        .and_then(|d| d.get("symbol"))
        .and_then(|s| s.as_array());

    match symbols {
        Some(s) if !s.is_empty() => print_symbol_xml(&s[0], true),
        _ => println!("<error>symbol '{}' not found</error>", symbol),
    }

    Ok(())
}

/// Context: Search + Read combined (supports multiple queries).
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

        if let Some(err) = get_graphql_error(&json) {
            println!("<error>{}</error>", err);
            continue;
        }

        let symbols = json
            .get("data")
            .and_then(|d| d.get("symbol"))
            .and_then(|s| s.as_array());

        match symbols {
            Some(s) if !s.is_empty() => {
                println!(
                    "<results query=\"{}\" count=\"{}\">",
                    query,
                    s.len().min(limit)
                );
                for sym in s.iter().take(limit) {
                    print_symbol_xml(sym, true);
                }
                println!("</results>");
            }
            _ => println!("<results query=\"{}\" count=\"0\"/>", query),
        }
    }

    Ok(())
}

/// Build/rebuild the code graph.
pub fn build(roots: &[PathBuf], cache_path: &Path) -> Result<()> {
    let root_refs: Vec<&Path> = roots.iter().map(|r| r.as_path()).collect();
    let graph = crate::graph::build_graph(&root_refs);
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

/// Get graph stats.
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

/// Show codebase overview — files grouped by directory with symbol counts.
pub fn overview(graph: &CodeGraph) -> Result<()> {
    let stats = graph.stats();
    println!("<overview>");
    println!("<files>{}</files>", stats.file_count);
    println!("<symbols>{}</symbols>", stats.symbol_count);
    println!("<edges>{}</edges>", stats.total_edges);

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

/// List all indexed files.
pub fn files(graph: &CodeGraph) -> Result<()> {
    let all_files = graph.all_files();
    println!("<files count=\"{}\">", all_files.len());
    for file_path in all_files {
        println!("<file>{}</file>", file_path.display());
    }
    println!("</files>");
    Ok(())
}

/// Show codebase map — compact view for AI agents.
pub fn map(graph: &CodeGraph, scope: Option<&str>) -> Result<()> {
    use std::collections::{BTreeMap, HashSet};

    let mut modules: BTreeMap<String, Vec<(String, String, usize, usize)>> = BTreeMap::new();
    let mut all_symbols: Vec<(String, String, usize, usize, String)> = Vec::new();

    for file_path in graph.all_files() {
        let dir = file_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        if let Some(s) = scope {
            if !dir.contains(s) && !file_path.to_string_lossy().contains(s) {
                continue;
            }
        }

        for symbol in graph.symbols_in_file(&file_path) {
            if matches!(
                symbol.kind,
                crate::graph::types::NodeKind::Import | crate::graph::types::NodeKind::File
            ) {
                continue;
            }

            let callers = graph.dependents(&symbol.name).len();
            let callees = graph.dependencies(&symbol.name).len();
            let short_module = dir.split('/').next_back().unwrap_or(&dir).to_string();

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

    println!("<modules>");
    for (dir, symbols) in &modules {
        let short_dir = dir.split('/').next_back().unwrap_or(dir);
        println!(
            "<module name=\"{}\" symbols=\"{}\"/>",
            short_dir,
            symbols.len()
        );
    }
    println!("</modules>");

    // Entry points: functions with 0 callers that have callees
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

    // Top connected symbols
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

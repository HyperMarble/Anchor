//! Read/Search operations: search, read, context
//!
//! Three commands for all read operations:
//! - search: Find symbols (lightweight, names + locations)
//! - read: Get full context for a symbol (code + relationships)
//! - context: Search + Read combined (find + full context)

use anyhow::Result;
use std::path::Path;

use crate::graph::CodeGraph;
use crate::query::graph_search;
use crate::regex::{parse as parse_regex, Matcher};

/// Search for symbols by name or pattern.
///
/// Lightweight - returns names, files, lines only.
/// Compact format for token efficiency.
pub fn search(graph: &CodeGraph, query: &str, pattern: Option<&str>, limit: usize) -> Result<()> {
    // If pattern provided, use regex matching
    if let Some(pat) = pattern {
        return search_with_pattern(graph, pat, limit);
    }

    let result = graph_search(graph, query, 0);

    if result.symbols.is_empty() && result.matched_files.is_empty() {
        println!("No results for '{}'", query);
        return Ok(());
    }

    // Compact format: name Kind file:line
    for sym in result.symbols.iter().take(limit) {
        let file_name = sym.file.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| sym.file.to_string_lossy().to_string());
        println!("{} {:?} {}:{}", sym.name, sym.kind, file_name, sym.line);
    }

    // Files matched
    for file in &result.matched_files {
        let file_name = file.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| file.to_string_lossy().to_string());
        println!("@ {}", file_name);
    }

    Ok(())
}

/// Search with regex pattern (Brzozowski derivatives - ReDoS safe)
fn search_with_pattern(graph: &CodeGraph, pattern: &str, limit: usize) -> Result<()> {
    let regex = parse_regex(pattern)
        .map_err(|e| anyhow::anyhow!("Invalid pattern: {}", e))?;
    let mut matcher = Matcher::new(regex);

    let all_symbols = graph.all_symbols();
    let matched: Vec<_> = all_symbols
        .into_iter()
        .filter(|s| matcher.is_match(&s.symbol))
        .take(limit)
        .collect();

    if matched.is_empty() {
        println!("No symbols match pattern '{}'", pattern);
        return Ok(());
    }

    // Compact format: name Kind file:line
    for sym in &matched {
        let file_name = sym.file.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| sym.file.to_string_lossy().to_string());
        println!("{} {:?} {}:{}", sym.symbol, sym.kind, file_name, sym.line_start);
    }

    Ok(())
}

/// Read full context for a symbol.
///
/// Returns: code, callers, callees, file, line.
/// Compact format for token efficiency.
pub fn read(graph: &CodeGraph, symbol: &str) -> Result<()> {
    let results = graph.search(symbol, 1);

    if results.is_empty() {
        println!("Symbol '{}' not found", symbol);
        return Ok(());
    }

    let sym = &results[0];
    let dependents = graph.dependents(symbol);
    let dependencies = graph.dependencies(symbol);

    // Compact format: symbol Kind file:line
    let file_name = sym.file.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| sym.file.to_string_lossy().to_string());

    println!("{} {:?} {}:{}", sym.symbol, sym.kind, file_name, sym.line_start);

    // Callers (who calls this) - unique names only, no files
    if !dependents.is_empty() {
        let mut callers: Vec<_> = dependents.iter()
            .filter(|d| !d.symbol.ends_with(".rs") && !d.symbol.ends_with(".py") && !d.symbol.ends_with(".js") && !d.symbol.ends_with(".ts"))
            .map(|d| d.symbol.as_str())
            .collect();
        callers.sort();
        callers.dedup();
        if !callers.is_empty() {
            println!("> {}", callers.join(" "));
        }
    }

    // Callees (what this calls) - unique names only, no files
    if !dependencies.is_empty() {
        let mut callees: Vec<_> = dependencies.iter()
            .filter(|d| !d.symbol.ends_with(".rs") && !d.symbol.ends_with(".py") && !d.symbol.ends_with(".js") && !d.symbol.ends_with(".ts"))
            .map(|d| d.symbol.as_str())
            .collect();
        callees.sort();
        callees.dedup();
        if !callees.is_empty() {
            println!("< {}", callees.join(" "));
        }
    }

    // Code
    println!("---");
    println!("{}", sym.code);

    Ok(())
}

/// Context: Search + Read combined.
///
/// Finds symbols matching query, returns full context for each.
/// Compact format for token efficiency.
pub fn context(graph: &CodeGraph, query: &str, limit: usize) -> Result<()> {
    let result = graph_search(graph, query, 1);

    if result.symbols.is_empty() {
        println!("No results for '{}'", query);
        return Ok(());
    }

    for (i, sym) in result.symbols.iter().take(limit).enumerate() {
        if i > 0 {
            println!("\n===");
        }

        let dependents = graph.dependents(&sym.name);
        let dependencies = graph.dependencies(&sym.name);

        // Header: symbol Kind file:line
        let file_name = sym.file.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| sym.file.to_string_lossy().to_string());
        println!("{} {:?} {}:{}", sym.name, sym.kind, file_name, sym.line);

        // Callers - unique names, no files
        if !dependents.is_empty() {
            let mut callers: Vec<_> = dependents.iter()
                .filter(|d| !d.symbol.ends_with(".rs") && !d.symbol.ends_with(".py") && !d.symbol.ends_with(".js") && !d.symbol.ends_with(".ts"))
                .map(|d| d.symbol.as_str())
                .collect();
            callers.sort();
            callers.dedup();
            if !callers.is_empty() {
                println!("> {}", callers.join(" "));
            }
        }

        // Callees - unique names, no files
        if !dependencies.is_empty() {
            let mut callees: Vec<_> = dependencies.iter()
                .filter(|d| !d.symbol.ends_with(".rs") && !d.symbol.ends_with(".py") && !d.symbol.ends_with(".js") && !d.symbol.ends_with(".ts"))
                .map(|d| d.symbol.as_str())
                .collect();
            callees.sort();
            callees.dedup();
            if !callees.is_empty() {
                println!("< {}", callees.join(" "));
            }
        }

        // Code
        println!("---");
        println!("{}", sym.code);
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

/// Get graph stats
pub fn stats(graph: &CodeGraph) -> Result<()> {
    let s = graph.stats();
    println!("files:{} symbols:{} edges:{} names:{}", s.file_count, s.symbol_count, s.total_edges, s.unique_symbol_names);
    Ok(())
}

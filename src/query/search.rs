//
//  search.rs
//  Anchor
//
//  Created by hak (tharun)
//

use crate::graph::{CodeGraph, GraphSearchResult};

use super::types::{
    DependencyResponse, FileSymbolEntry, FileSymbolsResponse, Query, SearchResponse, StatsResponse,
};

/// Search for symbols by name.
pub fn anchor_search(graph: &CodeGraph, query: Query) -> SearchResponse {
    let name = query.symbol_name();
    let limit = 5;

    let mut results = graph.search(name, limit);

    // Apply optional filters for structured queries
    if let Query::Structured { kind, file, .. } = &query {
        if let Some(kind_filter) = kind {
            let kind_lower = kind_filter.to_lowercase();
            results.retain(|r| r.kind.to_string() == kind_lower);
        }
        if let Some(file_filter) = file {
            results.retain(|r| r.file.to_string_lossy().contains(file_filter.as_str()));
        }
    }

    SearchResponse {
        found: !results.is_empty(),
        count: results.len(),
        results,
    }
}

/// Get dependencies and dependents for a symbol.
pub fn anchor_dependencies(graph: &CodeGraph, symbol: &str) -> DependencyResponse {
    DependencyResponse {
        symbol: symbol.to_string(),
        dependents: graph.dependents(symbol),
        dependencies: graph.dependencies(symbol),
    }
}

/// Get graph statistics.
pub fn anchor_stats(graph: &CodeGraph) -> StatsResponse {
    StatsResponse {
        stats: graph.stats(),
    }
}

/// Get all symbols in a file.
pub fn anchor_file_symbols(graph: &CodeGraph, file_path: &str) -> FileSymbolsResponse {
    use std::path::Path;

    let path = Path::new(file_path);
    let symbols = graph.symbols_in_file(path);

    let entries: Vec<FileSymbolEntry> = symbols
        .iter()
        .map(|node| FileSymbolEntry {
            name: node.name.clone(),
            kind: node.kind.to_string(),
            line_start: node.line_start,
            line_end: node.line_end,
            code: node.code_snippet.clone(),
        })
        .collect();

    FileSymbolsResponse {
        file: file_path.to_string(),
        found: !entries.is_empty(),
        symbols: entries,
    }
}

/// Graph-aware search with BFS traversal.
pub fn graph_search(graph: &CodeGraph, query: &str, depth: usize) -> GraphSearchResult {
    graph.search_graph(query, depth)
}

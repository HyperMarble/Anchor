//! Search and query interface for the Anchor code graph.
//!
//! Provides the high-level query API that MCP tools will expose.
//! Wraps CodeGraph operations with JSON-serializable responses.

use serde::{Deserialize, Serialize};

use crate::graph::{CodeGraph, DependencyInfo, GraphSearchResult, GraphStats, SearchResult};

/// Query input — supports both simple string and structured queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Query {
    /// Simple string query: "login"
    Simple(String),
    /// Structured query with optional filters.
    Structured {
        /// The symbol name to search for.
        symbol: String,
        /// Optional: filter by kind (e.g., "function", "struct").
        kind: Option<String>,
        /// Optional: filter by file path.
        file: Option<String>,
    },
}

impl Query {
    /// Extract the symbol name from any query format.
    pub fn symbol_name(&self) -> &str {
        match self {
            Query::Simple(s) => s.as_str(),
            Query::Structured { symbol, .. } => symbol.as_str(),
        }
    }
}

/// The response format for anchor_search.
/// Designed to give the AI everything it needs in one shot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    /// Whether the search found results.
    pub found: bool,
    /// Number of results.
    pub count: usize,
    /// The results (max 3).
    pub results: Vec<SearchResult>,
}

/// The response format for anchor_dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyResponse {
    /// The symbol queried.
    pub symbol: String,
    /// What depends on this symbol (incoming).
    pub dependents: Vec<DependencyInfo>,
    /// What this symbol depends on (outgoing).
    pub dependencies: Vec<DependencyInfo>,
}

/// The response format for anchor_stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub stats: GraphStats,
}

/// Execute an anchor_search query against the graph.
pub fn anchor_search(graph: &CodeGraph, query: Query) -> SearchResponse {
    let name = query.symbol_name();
    let limit = 3; // Max 3 results as per design

    let mut results = graph.search(name, limit);

    // Apply optional filters for structured queries
    if let Query::Structured { kind, file, .. } = &query {
        if let Some(kind_filter) = kind {
            let kind_lower = kind_filter.to_lowercase();
            results.retain(|r| r.kind.to_string() == kind_lower);
        }
        if let Some(file_filter) = file {
            results.retain(|r| {
                r.file
                    .to_string_lossy()
                    .contains(file_filter.as_str())
            });
        }
    }

    SearchResponse {
        found: !results.is_empty(),
        count: results.len(),
        results,
    }
}

/// Execute an anchor_dependencies query against the graph.
pub fn anchor_dependencies(graph: &CodeGraph, symbol: &str) -> DependencyResponse {
    let dependents = graph.dependents(symbol);
    let dependencies = graph.dependencies(symbol);

    DependencyResponse {
        symbol: symbol.to_string(),
        dependents,
        dependencies,
    }
}

/// Get graph statistics.
pub fn anchor_stats(graph: &CodeGraph) -> StatsResponse {
    StatsResponse {
        stats: graph.stats(),
    }
}

/// The response format for anchor_file_symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbolsResponse {
    /// The file queried.
    pub file: String,
    /// Whether any symbols were found.
    pub found: bool,
    /// Symbols in the file.
    pub symbols: Vec<FileSymbolEntry>,
}

/// A single symbol entry in a file listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbolEntry {
    pub name: String,
    pub kind: String,
    pub line_start: usize,
    pub line_end: usize,
    pub code: String,
}

/// Get all symbols defined in a specific file.
pub fn anchor_file_symbols(graph: &CodeGraph, file_path: &str) -> FileSymbolsResponse {
    use std::path::Path;

    // Try exact match first, then substring match
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

// =============================================================================
// get_context - The unified query tool for AI agents
// =============================================================================

/// The unified response for get_context.
/// Returns everything an AI agent needs in ONE call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    /// The query that was executed
    pub query: String,
    /// The intent used
    pub intent: String,
    /// Whether the query found results
    pub found: bool,
    /// Primary results (symbols found)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<ContextSymbol>,
    /// Dependents (who uses this) - included for modify/refactor/understand
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependents: Vec<DependencyInfo>,
    /// Dependencies (what this uses) - included for understand/refactor
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DependencyInfo>,
    /// File symbols - for file overview
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_symbols: Vec<FileSymbolEntry>,
    /// Project stats - for project overview
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<GraphStats>,
}

/// A symbol in the context response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSymbol {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
    pub code: String,
}

impl Default for ContextResponse {
    fn default() -> Self {
        Self {
            query: String::new(),
            intent: "find".to_string(),
            found: false,
            symbols: Vec::new(),
            dependents: Vec::new(),
            dependencies: Vec::new(),
            file_symbols: Vec::new(),
            stats: None,
        }
    }
}

/// The unified get_context query.
///
/// - query: symbol name OR file path
/// - intent: "find" | "understand" | "modify" | "refactor" | "overview"
///
/// The AI knows what it wants. It passes the intent directly.
pub fn get_context(graph: &CodeGraph, query: &str, intent: &str) -> ContextResponse {
    let mut response = ContextResponse {
        query: query.to_string(),
        intent: intent.to_string(),
        ..Default::default()
    };

    match intent {
        "overview" => {
            // File path -> list symbols in file
            // Otherwise -> project stats
            if query.contains('/') || query.contains('.') {
                let file_response = anchor_file_symbols(graph, query);
                response.found = file_response.found;
                response.file_symbols = file_response.symbols;
            } else {
                response.stats = Some(graph.stats());
                response.found = true;
            }
        }

        "find" => {
            // Just find where it is
            let search = anchor_search(graph, Query::Simple(query.to_string()));
            response.found = search.found;
            response.symbols = to_context_symbols(&search.results);
        }

        "understand" => {
            // Find it + what it uses + what uses it
            let search = anchor_search(graph, Query::Simple(query.to_string()));
            response.found = search.found;
            response.symbols = to_context_symbols(&search.results);

            let deps = anchor_dependencies(graph, query);
            response.dependencies = deps.dependencies;
            response.dependents = deps.dependents;
        }

        "modify" => {
            // Find it + what will be affected (dependents only)
            let search = anchor_search(graph, Query::Simple(query.to_string()));
            response.found = search.found;
            response.symbols = to_context_symbols(&search.results);

            let deps = anchor_dependencies(graph, query);
            response.dependents = deps.dependents;
        }

        "refactor" => {
            // Find it + full dependency graph both directions
            let search = anchor_search(graph, Query::Simple(query.to_string()));
            response.found = search.found;
            response.symbols = to_context_symbols(&search.results);

            let deps = anchor_dependencies(graph, query);
            response.dependents = deps.dependents;
            response.dependencies = deps.dependencies;
        }

        // Unknown intent defaults to find
        _ => {
            let search = anchor_search(graph, Query::Simple(query.to_string()));
            response.found = search.found;
            response.symbols = to_context_symbols(&search.results);
        }
    }

    response
}

fn to_context_symbols(results: &[SearchResult]) -> Vec<ContextSymbol> {
    results.iter().map(|r| ContextSymbol {
        name: r.symbol.clone(),
        kind: r.kind.to_string(),
        file: r.file.to_string_lossy().to_string(),
        line: r.line_start,
        code: r.code.clone(),
    }).collect()
}

// ─── Graph Search (The PROPER search) ─────────────────────────────────────────

/// Graph-aware search that ACTUALLY uses the graph.
///
/// - Matches file paths (fuzzy) OR symbol names
/// - BFS traverses connections to get related code
/// - Returns the subgraph, not just isolated matches
pub fn graph_search(graph: &CodeGraph, query: &str, depth: usize) -> GraphSearchResult {
    graph.search_graph(query, depth)
}

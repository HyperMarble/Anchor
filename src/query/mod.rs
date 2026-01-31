//! Query module — high-level search and dependency queries.
//!
//! Provides the API surface that MCP tools will use to query the code graph.

pub mod search;

pub use search::{
    anchor_dependencies, anchor_file_symbols, anchor_search, anchor_stats, get_context,
    graph_search, ContextResponse, ContextSymbol, DependencyResponse, FileSymbolsResponse,
    Query, SearchResponse, StatsResponse,
};

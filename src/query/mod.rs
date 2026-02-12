//! Query module — high-level search and context queries.
//!
//! The main interface for AI agents to query the code graph.
//!
//! ## Core API
//!
//! ```ignore
//! // Three intents - that's it
//! get_context(graph, "login", "explore")  // What is it? How does it work?
//! get_context(graph, "login", "change")   // I'm modifying - what breaks?
//! get_context(graph, "login", "create")   // Adding similar - show patterns
//! ```

pub mod context;
pub mod search;
pub mod slice;
pub mod types;

// Re-export the main API
pub use context::{get_context, get_context_for_change};
pub use types::{
    ContextResponse, DependencyResponse, Edit, FileSymbolEntry, FileSymbolsResponse, Param,
    Query, Reference, SearchResponse, Signature, StatsResponse, Symbol,
};

// Re-export search functions for backwards compatibility
pub use search::{
    anchor_dependencies, anchor_file_symbols, anchor_search, anchor_stats, graph_search,
};

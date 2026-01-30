//! Code graph module — the structural backbone of Anchor.
//!
//! Provides the graph data model, engine, query capabilities,
//! and directory scanning/building for the code graph.

pub mod builder;
pub mod engine;
pub mod persistence;
pub mod types;

pub use builder::{build_graph, rebuild_file, scan_stats, ScanStats};
pub use engine::{CodeGraph, DependencyInfo, GraphStats, SearchResult, SymbolRef};
pub use types::{
    EdgeData, EdgeKind, ExtractedCall, ExtractedImport, ExtractedSymbol, FileExtractions, NodeData,
    NodeKind,
};

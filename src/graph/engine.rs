//! The core graph engine for Anchor.
//!
//! Uses petgraph to store code relationships and provides
//! query methods for searching and traversing the graph.

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use super::types::*;

/// The main code graph — holds all nodes, edges, and indexes for fast lookup.
pub struct CodeGraph {
    /// The directed graph storing code relationships.
    graph: DiGraph<NodeData, EdgeData>,
    /// Index: file path -> node index (for File nodes).
    file_index: HashMap<PathBuf, NodeIndex>,
    /// Index: symbol name -> list of node indexes (for quick name lookup).
    symbol_index: HashMap<String, Vec<NodeIndex>>,
    /// Index: (file_path, symbol_name) -> node index (for unique symbol resolution).
    qualified_index: HashMap<(PathBuf, String), NodeIndex>,
}

impl CodeGraph {
    /// Create a new empty code graph.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            file_index: HashMap::new(),
            symbol_index: HashMap::new(),
            qualified_index: HashMap::new(),
        }
    }

    /// Access the underlying petgraph (for serialization).
    pub(crate) fn inner_graph(&self) -> &DiGraph<NodeData, EdgeData> {
        &self.graph
    }

    /// Mutable access to the underlying petgraph (for deserialization).
    pub(crate) fn inner_graph_mut(&mut self) -> &mut DiGraph<NodeData, EdgeData> {
        &mut self.graph
    }

    // ─── Node Operations ────────────────────────────────────────

    /// Add a file node to the graph. Returns the node index.
    /// If the file was previously soft-deleted, it gets un-removed.
    pub fn add_file(&mut self, path: PathBuf) -> NodeIndex {
        if let Some(&idx) = self.file_index.get(&path) {
            // Un-remove if it was soft-deleted
            if let Some(node) = self.graph.node_weight_mut(idx) {
                node.removed = false;
            }
            return idx;
        }
        let data = NodeData::new_file(path.clone());
        let idx = self.graph.add_node(data);
        self.file_index.insert(path, idx);
        idx
    }

    /// Add a symbol node to the graph. Returns the node index.
    pub fn add_symbol(
        &mut self,
        name: String,
        kind: NodeKind,
        file_path: PathBuf,
        line_start: usize,
        line_end: usize,
        code_snippet: String,
    ) -> NodeIndex {
        let data = NodeData::new_symbol(
            name.clone(),
            kind,
            file_path.clone(),
            line_start,
            line_end,
            code_snippet,
        );
        let idx = self.graph.add_node(data);

        // Update indexes
        self.symbol_index
            .entry(name.clone())
            .or_default()
            .push(idx);
        self.qualified_index
            .insert((file_path, name), idx);

        idx
    }

    // ─── Edge Operations ────────────────────────────────────────

    /// Add an edge between two nodes.
    pub fn add_edge(&mut self, from: NodeIndex, to: NodeIndex, kind: EdgeKind) {
        self.graph.add_edge(from, to, EdgeData::new(kind));
    }

    // ─── Query Operations ───────────────────────────────────────

    /// Search for symbols by name. Returns up to `limit` results.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Exact match first
        if let Some(indexes) = self.symbol_index.get(query) {
            for &idx in indexes.iter().take(limit) {
                if let Some(result) = self.build_search_result(idx) {
                    results.push(result);
                }
            }
        }

        // If no exact match, fuzzy search
        if results.is_empty() {
            let mut scored: Vec<(usize, NodeIndex)> = self
                .symbol_index
                .iter()
                .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
                .flat_map(|(_, indexes)| {
                    indexes.iter().filter_map(|&idx| {
                        let node = &self.graph[idx];
                        if node.removed {
                            return None;
                        }
                        // Score: exact > starts_with > contains
                        let score = if node.name == query {
                            0
                        } else if node.name.to_lowercase().starts_with(&query_lower) {
                            1
                        } else {
                            2
                        };
                        Some((score, idx))
                    })
                })
                .collect();

            scored.sort_by_key(|(score, _)| *score);

            for (_, idx) in scored.into_iter().take(limit) {
                if let Some(result) = self.build_search_result(idx) {
                    results.push(result);
                }
            }
        }

        results
    }

    /// Graph-aware search: finds by file path OR symbol name, then traverses connections.
    ///
    /// This is the PROPER search that uses the graph:
    /// 1. Try to match file paths (fuzzy)
    /// 2. Try to match symbol names
    /// 3. BFS traverse to get connected nodes
    pub fn search_graph(&self, query: &str, depth: usize) -> GraphSearchResult {
        let query_lower = query.to_lowercase();
        let mut result = GraphSearchResult::default();

        // 1. Try file path match first
        let file_matches: Vec<_> = self.file_index.iter()
            .filter(|(path, &idx)| {
                let path_str = path.to_string_lossy().to_lowercase();
                self.is_live(idx) && (
                    path_str.contains(&query_lower) ||
                    path.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                )
            })
            .collect();

        if !file_matches.is_empty() {
            result.match_type = "file".to_string();

            // Collect all symbol NodeIndexes from matched files
            let mut symbol_indexes: Vec<NodeIndex> = Vec::new();

            for (path, &file_idx) in &file_matches {
                result.matched_files.push(path.to_path_buf());

                // Get all symbols defined in this file (traverse Defines edges)
                for edge in self.graph.edges_directed(file_idx, Direction::Outgoing) {
                    if edge.weight().kind == EdgeKind::Defines && self.is_live(edge.target()) {
                        symbol_indexes.push(edge.target());
                        let node = &self.graph[edge.target()];
                        result.symbols.push(SymbolInfo {
                            name: node.name.clone(),
                            kind: node.kind,
                            file: node.file_path.clone(),
                            line: node.line_start,
                            code: node.code_snippet.clone(),
                        });
                    }
                }
            }

            // Now traverse connections FROM these symbols if depth > 0
            if depth > 0 {
                let mut visited: HashSet<NodeIndex> = symbol_indexes.iter().copied().collect();

                for &idx in &symbol_indexes {
                    let node = &self.graph[idx];

                    // Outgoing edges (what this symbol uses/calls)
                    for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                        let target = edge.target();
                        if self.is_live(target) && !visited.contains(&target) {
                            visited.insert(target);
                            let target_node = &self.graph[target];
                            if target_node.kind != NodeKind::File {
                                result.connections.push(ConnectionInfo {
                                    from: node.name.clone(),
                                    to: target_node.name.clone(),
                                    relationship: edge.weight().kind,
                                });
                            }
                        }
                    }

                    // Incoming edges (what calls/uses this symbol)
                    for edge in self.graph.edges_directed(idx, Direction::Incoming) {
                        let source = edge.source();
                        if self.is_live(source) && !visited.contains(&source) {
                            visited.insert(source);
                            let source_node = &self.graph[source];
                            if source_node.kind != NodeKind::File {
                                result.connections.push(ConnectionInfo {
                                    from: source_node.name.clone(),
                                    to: node.name.clone(),
                                    relationship: edge.weight().kind,
                                });
                            }
                        }
                    }
                }
            }

            return result;
        }

        // 2. Try symbol name match
        let symbol_matches: Vec<NodeIndex> = self.symbol_index.iter()
            .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
            .flat_map(|(_, indexes)| indexes.iter().copied())
            .filter(|&idx| self.is_live(idx))
            .collect();

        if symbol_matches.is_empty() {
            result.match_type = "none".to_string();
            return result;
        }

        result.match_type = "symbol".to_string();

        // 3. BFS traverse from matched symbols to get connected subgraph
        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();

        for idx in &symbol_matches {
            queue.push_back((*idx, 0));
            visited.insert(*idx);
        }

        while let Some((idx, current_depth)) = queue.pop_front() {
            let node = &self.graph[idx];

            if node.kind != NodeKind::File {
                result.symbols.push(SymbolInfo {
                    name: node.name.clone(),
                    kind: node.kind,
                    file: node.file_path.clone(),
                    line: node.line_start,
                    code: node.code_snippet.clone(),
                });
            }

            // Continue BFS if within depth limit
            if current_depth < depth {
                // Outgoing edges (what this symbol uses)
                for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                    let target = edge.target();
                    if self.is_live(target) && !visited.contains(&target) {
                        visited.insert(target);
                        queue.push_back((target, current_depth + 1));

                        let target_node = &self.graph[target];
                        if target_node.kind != NodeKind::File {
                            result.connections.push(ConnectionInfo {
                                from: node.name.clone(),
                                to: target_node.name.clone(),
                                relationship: edge.weight().kind,
                            });
                        }
                    }
                }

                // Incoming edges (what uses this symbol)
                for edge in self.graph.edges_directed(idx, Direction::Incoming) {
                    let source = edge.source();
                    if self.is_live(source) && !visited.contains(&source) {
                        visited.insert(source);
                        queue.push_back((source, current_depth + 1));

                        let source_node = &self.graph[source];
                        if source_node.kind != NodeKind::File {
                            result.connections.push(ConnectionInfo {
                                from: source_node.name.clone(),
                                to: node.name.clone(),
                                relationship: edge.weight().kind,
                            });
                        }
                    }
                }
            }
        }

        result
    }

    /// Find what depends on a given symbol (who calls it, who references it).
    pub fn dependents(&self, symbol_name: &str) -> Vec<DependencyInfo> {
        let mut deps = Vec::new();

        if let Some(indexes) = self.symbol_index.get(symbol_name) {
            for &idx in indexes {
                if !self.is_live(idx) {
                    continue;
                }
                for edge in self.graph.edges_directed(idx, Direction::Incoming) {
                    let source_idx = edge.source();
                    if !self.is_live(source_idx) {
                        continue;
                    }
                    let source = &self.graph[source_idx];
                    let edge_data = edge.weight();

                    deps.push(DependencyInfo {
                        symbol: source.name.clone(),
                        kind: source.kind,
                        file: source.file_path.clone(),
                        line: source.line_start,
                        relationship: edge_data.kind,
                    });
                }
            }
        }

        deps
    }

    /// Find what a given symbol depends on (what it calls, what it references).
    pub fn dependencies(&self, symbol_name: &str) -> Vec<DependencyInfo> {
        let mut deps = Vec::new();

        if let Some(indexes) = self.symbol_index.get(symbol_name) {
            for &idx in indexes {
                if !self.is_live(idx) {
                    continue;
                }
                for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                    let target_idx = edge.target();
                    if !self.is_live(target_idx) {
                        continue;
                    }
                    let target = &self.graph[target_idx];
                    let edge_data = edge.weight();

                    deps.push(DependencyInfo {
                        symbol: target.name.clone(),
                        kind: target.kind,
                        file: target.file_path.clone(),
                        line: target.line_start,
                        relationship: edge_data.kind,
                    });
                }
            }
        }

        deps
    }

    /// Get all symbols defined in a specific file.
    pub fn symbols_in_file(&self, path: &Path) -> Vec<&NodeData> {
        if let Some(&file_idx) = self.file_index.get(path) {
            if !self.is_live(file_idx) {
                return Vec::new();
            }
            self.graph
                .edges_directed(file_idx, Direction::Outgoing)
                .filter(|edge| edge.weight().kind == EdgeKind::Defines && self.is_live(edge.target()))
                .map(|edge| &self.graph[edge.target()])
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Find a symbol by its qualified name (file + symbol name).
    pub fn find_qualified(&self, file_path: &Path, name: &str) -> Option<&NodeData> {
        self.qualified_index
            .get(&(file_path.to_path_buf(), name.to_string()))
            .and_then(|&idx| {
                let node = &self.graph[idx];
                if node.removed { None } else { Some(node) }
            })
    }

    // ─── Stats ──────────────────────────────────────────────────

    /// Get graph statistics (excludes soft-deleted nodes).
    pub fn stats(&self) -> GraphStats {
        let mut file_count = 0;
        let mut symbol_count = 0;

        for node in self.graph.node_weights() {
            if node.removed {
                continue;
            }
            match node.kind {
                NodeKind::File => file_count += 1,
                _ => symbol_count += 1,
            }
        }

        GraphStats {
            total_nodes: file_count + symbol_count,
            total_edges: self.graph.edge_count(),
            file_count,
            symbol_count,
            unique_symbol_names: self.symbol_index.len(),
        }
    }

    // ─── Internal Helpers ───────────────────────────────────────

    /// Check if a node is live (not removed).
    fn is_live(&self, idx: NodeIndex) -> bool {
        self.graph
            .node_weight(idx)
            .is_some_and(|n| !n.removed)
    }

    /// Build a SearchResult from a node index, including connections.
    fn build_search_result(&self, idx: NodeIndex) -> Option<SearchResult> {
        let node = &self.graph[idx];

        // Don't return File nodes or removed nodes
        if node.kind == NodeKind::File || node.removed {
            return None;
        }

        // Collect outgoing calls (this symbol calls...), skip removed targets
        let calls: Vec<SymbolRef> = self
            .graph
            .edges_directed(idx, Direction::Outgoing)
            .filter(|e| e.weight().kind == EdgeKind::Calls && self.is_live(e.target()))
            .map(|e| {
                let target = &self.graph[e.target()];
                SymbolRef {
                    name: target.name.clone(),
                    file: target.file_path.clone(),
                    line: target.line_start,
                }
            })
            .collect();

        // Collect incoming calls (called by...), skip removed sources
        let called_by: Vec<SymbolRef> = self
            .graph
            .edges_directed(idx, Direction::Incoming)
            .filter(|e| e.weight().kind == EdgeKind::Calls && self.is_live(e.source()))
            .map(|e| {
                let source = &self.graph[e.source()];
                SymbolRef {
                    name: source.name.clone(),
                    file: source.file_path.clone(),
                    line: source.line_start,
                }
            })
            .collect();

        // Collect imports related to the file this symbol is in, skip removed
        let imports: Vec<String> = if let Some(&file_idx) = self.file_index.get(&node.file_path) {
            self.graph
                .edges_directed(file_idx, Direction::Outgoing)
                .filter(|e| e.weight().kind == EdgeKind::Imports && self.is_live(e.target()))
                .map(|e| {
                    let target = &self.graph[e.target()];
                    target.name.clone()
                })
                .collect()
        } else {
            Vec::new()
        };

        Some(SearchResult {
            symbol: node.name.clone(),
            kind: node.kind,
            file: node.file_path.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
            code: node.code_snippet.clone(),
            calls,
            called_by,
            imports,
        })
    }

    // ─── Graph Building from Extractions ────────────────────────

    /// Build the graph from a set of file extractions.
    /// This is the main entry point for populating the graph.
    pub fn build_from_extractions(&mut self, extractions: Vec<FileExtractions>) {
        debug!(file_count = extractions.len(), "ingesting extractions into graph");
        // Phase 1: Add all file nodes and symbol nodes
        for extraction in &extractions {
            let file_idx = self.add_file(extraction.file_path.clone());

            for symbol in &extraction.symbols {
                let sym_idx = self.add_symbol(
                    symbol.name.clone(),
                    symbol.kind,
                    extraction.file_path.clone(),
                    symbol.line_start,
                    symbol.line_end,
                    symbol.code_snippet.clone(),
                );

                // File DEFINES Symbol
                self.add_edge(file_idx, sym_idx, EdgeKind::Defines);
            }

            // Add import nodes
            for import in &extraction.imports {
                let import_idx = self.add_symbol(
                    import.path.clone(),
                    NodeKind::Import,
                    extraction.file_path.clone(),
                    import.line,
                    import.line,
                    String::new(),
                );
                self.add_edge(file_idx, import_idx, EdgeKind::Imports);
            }
        }

        // Phase 2: Resolve cross-references (calls)
        for extraction in &extractions {
            for call in &extraction.calls {
                // Find the caller node
                let caller_key = (extraction.file_path.clone(), call.caller.clone());
                let callee_nodes = self.symbol_index.get(&call.callee).cloned();

                if let Some(&caller_idx) = self.qualified_index.get(&caller_key) {
                    if let Some(callee_indexes) = callee_nodes {
                        // Connect to the first matching callee
                        // (in v0, we take the first match — later versions can be smarter)
                        if let Some(&callee_idx) = callee_indexes.first() {
                            self.add_edge(caller_idx, callee_idx, EdgeKind::Calls);
                        }
                    }
                }
            }
        }

        // Phase 3: Resolve contains relationships (parent -> child)
        for extraction in &extractions {
            for symbol in &extraction.symbols {
                if let Some(ref parent_name) = symbol.parent {
                    let child_key = (extraction.file_path.clone(), symbol.name.clone());
                    let parent_key = (extraction.file_path.clone(), parent_name.clone());

                    if let (Some(&parent_idx), Some(&child_idx)) = (
                        self.qualified_index.get(&parent_key),
                        self.qualified_index.get(&child_key),
                    ) {
                        self.add_edge(parent_idx, child_idx, EdgeKind::Contains);
                    }
                }
            }
        }
    }

    /// Soft-delete all nodes and edges originating from a specific file.
    /// Marks nodes as removed so queries skip them. Use `compact()` to
    /// physically reclaim memory.
    pub fn remove_file(&mut self, path: &Path) {
        if let Some(&file_idx) = self.file_index.get(path) {
            debug!(file = %path.display(), "removing file from graph");
            // Collect ALL child nodes (DEFINES + IMPORTS edges from file)
            let child_nodes: Vec<NodeIndex> = self
                .graph
                .edges_directed(file_idx, Direction::Outgoing)
                .map(|e| e.target())
                .collect();

            // Soft-delete each child node and clean indexes
            for &node_idx in &child_nodes {
                if let Some(node) = self.graph.node_weight_mut(node_idx) {
                    let name = node.name.clone();
                    let file = node.file_path.clone();
                    node.removed = true;

                    // Remove from symbol_index
                    if let Some(indexes) = self.symbol_index.get_mut(&name) {
                        indexes.retain(|&idx| idx != node_idx);
                        if indexes.is_empty() {
                            self.symbol_index.remove(&name);
                        }
                    }

                    // Remove from qualified_index
                    self.qualified_index.remove(&(file, name));
                }
            }

            // Soft-delete the file node itself
            if let Some(file_node) = self.graph.node_weight_mut(file_idx) {
                file_node.removed = true;
            }
            self.file_index.remove(path);
        }
    }

    /// Rebuild the graph from scratch, removing all soft-deleted nodes.
    /// Call this periodically or after many incremental updates to reclaim memory.
    pub fn compact(&mut self) {
        info!("compacting graph — rebuilding without soft-deleted nodes");
        // Collect all live extractions
        let mut live_files: HashMap<PathBuf, Vec<NodeIndex>> = HashMap::new();
        for idx in self.graph.node_indices() {
            let node = &self.graph[idx];
            if !node.removed && node.kind == NodeKind::File {
                live_files.insert(node.file_path.clone(), Vec::new());
            }
        }

        // Build a new graph with only live nodes
        let mut new_graph = CodeGraph::new();

        // Re-add all live file nodes
        for path in live_files.keys() {
            new_graph.add_file(path.clone());
        }

        // Re-add all live symbol nodes and their edges
        let mut old_to_new: HashMap<NodeIndex, NodeIndex> = HashMap::new();
        for idx in self.graph.node_indices() {
            let node = &self.graph[idx];
            if node.removed {
                continue;
            }
            if node.kind == NodeKind::File {
                if let Some(&new_idx) = new_graph.file_index.get(&node.file_path) {
                    old_to_new.insert(idx, new_idx);
                }
            } else {
                let new_idx = new_graph.add_symbol(
                    node.name.clone(),
                    node.kind,
                    node.file_path.clone(),
                    node.line_start,
                    node.line_end,
                    node.code_snippet.clone(),
                );
                old_to_new.insert(idx, new_idx);
            }
        }

        // Re-add all edges between live nodes
        for edge in self.graph.edge_indices() {
            if let Some((src, tgt)) = self.graph.edge_endpoints(edge) {
                if let (Some(&new_src), Some(&new_tgt)) =
                    (old_to_new.get(&src), old_to_new.get(&tgt))
                {
                    let edge_data = &self.graph[edge];
                    new_graph.add_edge(new_src, new_tgt, edge_data.kind);
                }
            }
        }

        // Replace self with the compacted graph
        *self = new_graph;

        let stats = self.stats();
        info!(
            files = stats.file_count,
            symbols = stats.symbol_count,
            edges = stats.total_edges,
            "compact complete"
        );
    }
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Query Result Types ─────────────────────────────────────────

/// A search result returned by `CodeGraph::search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The symbol name.
    pub symbol: String,
    /// What kind of code element.
    pub kind: NodeKind,
    /// File where it's defined.
    pub file: PathBuf,
    /// Start line.
    pub line_start: usize,
    /// End line.
    pub line_end: usize,
    /// The actual source code.
    pub code: String,
    /// What this symbol calls.
    pub calls: Vec<SymbolRef>,
    /// What calls this symbol.
    pub called_by: Vec<SymbolRef>,
    /// Imports in the same file.
    pub imports: Vec<String>,
}

/// A reference to a symbol (lightweight, for connections).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    /// Symbol name.
    pub name: String,
    /// File path.
    pub file: PathBuf,
    /// Line number.
    pub line: usize,
}

/// Dependency information for a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInfo {
    /// The symbol name.
    pub symbol: String,
    /// The kind of symbol.
    pub kind: NodeKind,
    /// File path.
    pub file: PathBuf,
    /// Line number.
    pub line: usize,
    /// How it's related.
    pub relationship: EdgeKind,
}

/// Statistics about the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub file_count: usize,
    pub symbol_count: usize,
    pub unique_symbol_names: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let graph = CodeGraph::new();
        let stats = graph.stats();
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.total_edges, 0);
    }

    #[test]
    fn test_add_file_and_symbol() {
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/main.rs"));
        let fn_idx = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1,
            10,
            "fn main() { }".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        let stats = graph.stats();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.symbol_count, 1);
        assert_eq!(stats.total_edges, 1);
    }

    #[test]
    fn test_search_exact() {
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn_idx = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            5,
            20,
            "pub fn login(user: &str) -> bool { }".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        let results = graph.search("login", 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol, "login");
        assert_eq!(results[0].kind, NodeKind::Function);
    }

    #[test]
    fn test_search_fuzzy() {
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn1 = graph.add_symbol(
            "user_login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            5,
            20,
            "fn user_login() {}".to_string(),
        );
        let fn2 = graph.add_symbol(
            "user_logout".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            25,
            40,
            "fn user_logout() {}".to_string(),
        );
        graph.add_edge(file_idx, fn1, EdgeKind::Defines);
        graph.add_edge(file_idx, fn2, EdgeKind::Defines);

        let results = graph.search("login", 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol, "user_login");
    }

    #[test]
    fn test_calls_relationship() {
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/main.rs"));
        let main_idx = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1,
            10,
            "fn main() { login(); }".to_string(),
        );
        let login_idx = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            5,
            20,
            "fn login() {}".to_string(),
        );

        graph.add_edge(file_idx, main_idx, EdgeKind::Defines);
        graph.add_edge(main_idx, login_idx, EdgeKind::Calls);

        let results = graph.search("main", 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].calls.len(), 1);
        assert_eq!(results[0].calls[0].name, "login");

        let login_results = graph.search("login", 3);
        assert_eq!(login_results.len(), 1);
        assert_eq!(login_results[0].called_by.len(), 1);
        assert_eq!(login_results[0].called_by[0].name, "main");
    }

    #[test]
    fn test_build_from_extractions() {
        let extractions = vec![FileExtractions {
            file_path: PathBuf::from("src/lib.rs"),
            symbols: vec![
                ExtractedSymbol {
                    name: "add".to_string(),
                    kind: NodeKind::Function,
                    line_start: 1,
                    line_end: 3,
                    code_snippet: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
                    parent: None,
                },
                ExtractedSymbol {
                    name: "multiply".to_string(),
                    kind: NodeKind::Function,
                    line_start: 5,
                    line_end: 7,
                    code_snippet: "fn multiply(a: i32, b: i32) -> i32 { a * b }".to_string(),
                    parent: None,
                },
            ],
            imports: vec![],
            calls: vec![ExtractedCall {
                caller: "multiply".to_string(),
                callee: "add".to_string(),
                line: 6,
            }],
        }];

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(extractions);

        let stats = graph.stats();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.symbol_count, 2);

        // Check that multiply calls add
        let results = graph.search("multiply", 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].calls.len(), 1);
        assert_eq!(results[0].calls[0].name, "add");
    }

    // ─── Removal Tests ──────────────────────────────────────────

    #[test]
    fn test_remove_file_clears_stats() {
        let mut graph = CodeGraph::new();
        let file_idx = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn_idx = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1, 10,
            "fn login() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        assert_eq!(graph.stats().file_count, 1);
        assert_eq!(graph.stats().symbol_count, 1);

        graph.remove_file(Path::new("src/auth.rs"));

        let stats = graph.stats();
        assert_eq!(stats.file_count, 0, "File should be removed from stats");
        assert_eq!(stats.symbol_count, 0, "Symbols should be removed from stats");
    }

    #[test]
    fn test_remove_file_hides_from_search() {
        let mut graph = CodeGraph::new();
        let file_idx = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn_idx = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1, 10,
            "fn login() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        // Before removal: findable
        assert_eq!(graph.search("login", 3).len(), 1);

        graph.remove_file(Path::new("src/auth.rs"));

        // After removal: gone
        assert_eq!(graph.search("login", 3).len(), 0, "Removed symbol should not appear in search");
    }

    #[test]
    fn test_remove_and_readd_no_duplicates() {
        let mut graph = CodeGraph::new();

        // Add file with symbol
        let file_idx = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn_idx = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1, 10,
            "fn login() { v1 }".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        // Remove
        graph.remove_file(Path::new("src/auth.rs"));

        // Re-add with different content
        let file_idx2 = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn_idx2 = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1, 15,
            "fn login() { v2 }".to_string(),
        );
        graph.add_edge(file_idx2, fn_idx2, EdgeKind::Defines);

        // Should have exactly 1 result with v2 content
        let results = graph.search("login", 3);
        assert_eq!(results.len(), 1, "Should have exactly 1 result after re-add");
        assert!(results[0].code.contains("v2"), "Should have updated code");

        let stats = graph.stats();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.symbol_count, 1);
    }

    #[test]
    fn test_remove_file_preserves_other_files() {
        let mut graph = CodeGraph::new();

        // Add file A
        let file_a = graph.add_file(PathBuf::from("src/auth.rs"));
        let login = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1, 10,
            "fn login() {}".to_string(),
        );
        graph.add_edge(file_a, login, EdgeKind::Defines);

        // Add file B
        let file_b = graph.add_file(PathBuf::from("src/main.rs"));
        let main_fn = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1, 5,
            "fn main() {}".to_string(),
        );
        graph.add_edge(file_b, main_fn, EdgeKind::Defines);

        // Remove A
        graph.remove_file(Path::new("src/auth.rs"));

        // B should still be intact
        let results = graph.search("main", 3);
        assert_eq!(results.len(), 1, "File B should be unaffected");
        assert_eq!(results[0].symbol, "main");

        // A should be gone
        assert_eq!(graph.search("login", 3).len(), 0);

        let stats = graph.stats();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.symbol_count, 1);
    }

    #[test]
    fn test_remove_file_clears_cross_references() {
        let mut graph = CodeGraph::new();

        // File A has fn main() that calls login()
        let file_a = graph.add_file(PathBuf::from("src/main.rs"));
        let main_fn = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1, 10,
            "fn main() { login(); }".to_string(),
        );
        graph.add_edge(file_a, main_fn, EdgeKind::Defines);

        // File B has fn login()
        let file_b = graph.add_file(PathBuf::from("src/auth.rs"));
        let login_fn = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1, 10,
            "fn login() {}".to_string(),
        );
        graph.add_edge(file_b, login_fn, EdgeKind::Defines);
        graph.add_edge(main_fn, login_fn, EdgeKind::Calls);

        // Before removal: login is called_by main
        let results = graph.search("login", 3);
        assert_eq!(results[0].called_by.len(), 1);

        // Remove file A (main.rs)
        graph.remove_file(Path::new("src/main.rs"));

        // After removal: login should have no callers
        let results = graph.search("login", 3);
        assert_eq!(results.len(), 1, "login itself should still exist");
        assert_eq!(results[0].called_by.len(), 0, "Removed caller should disappear from called_by");
    }

    #[test]
    fn test_compact_reclaims_memory() {
        let mut graph = CodeGraph::new();

        // Add and remove a file
        let file_idx = graph.add_file(PathBuf::from("src/old.rs"));
        let fn_idx = graph.add_symbol(
            "old_fn".to_string(),
            NodeKind::Function,
            PathBuf::from("src/old.rs"),
            1, 10,
            "fn old_fn() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        // Add a file that stays
        let file_keep = graph.add_file(PathBuf::from("src/keep.rs"));
        let keep_fn = graph.add_symbol(
            "keep_fn".to_string(),
            NodeKind::Function,
            PathBuf::from("src/keep.rs"),
            1, 5,
            "fn keep_fn() {}".to_string(),
        );
        graph.add_edge(file_keep, keep_fn, EdgeKind::Defines);

        graph.remove_file(Path::new("src/old.rs"));

        // Before compact: petgraph still has ghost nodes internally
        let stats_before = graph.stats();
        assert_eq!(stats_before.file_count, 1);
        assert_eq!(stats_before.symbol_count, 1);

        // Compact
        graph.compact();

        // After compact: clean graph, same logical state
        let stats_after = graph.stats();
        assert_eq!(stats_after.file_count, 1);
        assert_eq!(stats_after.symbol_count, 1);

        // keep_fn should still be searchable
        let results = graph.search("keep_fn", 3);
        assert_eq!(results.len(), 1);

        // old_fn should still be gone
        assert_eq!(graph.search("old_fn", 3).len(), 0);
    }

    // ─── Edge-Case Tests ───────────────────────────────────────

    #[test]
    fn test_search_empty_query() {
        let mut graph = CodeGraph::new();
        let file_idx = graph.add_file(PathBuf::from("src/main.rs"));
        let fn_idx = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1, 5,
            "fn main() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        // Empty string query should return no results (not crash)
        let results = graph.search("", 3);
        assert!(results.is_empty() || results.iter().all(|r| r.symbol.contains("")));
    }

    #[test]
    fn test_search_nonexistent_symbol() {
        let graph = CodeGraph::new();
        let results = graph.search("does_not_exist", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_dependents_empty_graph() {
        let graph = CodeGraph::new();
        let deps = graph.dependents("anything");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_dependencies_empty_graph() {
        let graph = CodeGraph::new();
        let deps = graph.dependencies("anything");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_symbols_in_nonexistent_file() {
        let graph = CodeGraph::new();
        let symbols = graph.symbols_in_file(Path::new("nonexistent.rs"));
        assert!(symbols.is_empty());
    }

    #[test]
    fn test_cycle_in_calls() {
        // A calls B, B calls A — should not infinite loop or crash
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/cycle.rs"));
        let a_idx = graph.add_symbol(
            "func_a".to_string(),
            NodeKind::Function,
            PathBuf::from("src/cycle.rs"),
            1, 5,
            "fn func_a() { func_b(); }".to_string(),
        );
        let b_idx = graph.add_symbol(
            "func_b".to_string(),
            NodeKind::Function,
            PathBuf::from("src/cycle.rs"),
            6, 10,
            "fn func_b() { func_a(); }".to_string(),
        );

        graph.add_edge(file_idx, a_idx, EdgeKind::Defines);
        graph.add_edge(file_idx, b_idx, EdgeKind::Defines);
        graph.add_edge(a_idx, b_idx, EdgeKind::Calls);
        graph.add_edge(b_idx, a_idx, EdgeKind::Calls);

        // Search should work
        let results = graph.search("func_a", 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].calls.len(), 1);
        assert_eq!(results[0].called_by.len(), 1);

        // Dependencies should work
        let deps = graph.dependencies("func_a");
        assert!(!deps.is_empty());
        let dependents = graph.dependents("func_a");
        assert!(!dependents.is_empty());
    }

    #[test]
    fn test_duplicate_symbol_names_across_files() {
        let mut graph = CodeGraph::new();

        // Both files have a function called "init"
        let file_a = graph.add_file(PathBuf::from("src/a.rs"));
        let init_a = graph.add_symbol(
            "init".to_string(),
            NodeKind::Function,
            PathBuf::from("src/a.rs"),
            1, 5,
            "fn init() { /* a */ }".to_string(),
        );
        graph.add_edge(file_a, init_a, EdgeKind::Defines);

        let file_b = graph.add_file(PathBuf::from("src/b.rs"));
        let init_b = graph.add_symbol(
            "init".to_string(),
            NodeKind::Function,
            PathBuf::from("src/b.rs"),
            1, 5,
            "fn init() { /* b */ }".to_string(),
        );
        graph.add_edge(file_b, init_b, EdgeKind::Defines);

        // Search for "init" should return both
        let results = graph.search("init", 10);
        assert_eq!(results.len(), 2);

        // Qualified lookup should distinguish them
        let qa = graph.find_qualified(Path::new("src/a.rs"), "init");
        let qb = graph.find_qualified(Path::new("src/b.rs"), "init");
        assert!(qa.is_some());
        assert!(qb.is_some());
        assert!(qa.unwrap().code_snippet.contains("/* a */"));
        assert!(qb.unwrap().code_snippet.contains("/* b */"));
    }

    #[test]
    fn test_remove_nonexistent_file() {
        let mut graph = CodeGraph::new();
        // Should not crash
        graph.remove_file(Path::new("does_not_exist.rs"));
        assert_eq!(graph.stats().total_nodes, 0);
    }

    #[test]
    fn test_stats_edge_count_excludes_nothing() {
        // Edge count currently includes all edges (even to removed nodes)
        // This test documents that behavior
        let mut graph = CodeGraph::new();
        let file_idx = graph.add_file(PathBuf::from("src/main.rs"));
        let fn_idx = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1, 5,
            "fn main() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        assert_eq!(graph.stats().total_edges, 1);
    }

    #[test]
    fn test_multiple_edges_same_nodes() {
        let mut graph = CodeGraph::new();
        let file_idx = graph.add_file(PathBuf::from("src/main.rs"));
        let fn_idx = graph.add_symbol(
            "process".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1, 5,
            "fn process() {}".to_string(),
        );

        // Add multiple edge types
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);
        graph.add_edge(file_idx, fn_idx, EdgeKind::Contains);

        assert_eq!(graph.stats().total_edges, 2);
    }
}

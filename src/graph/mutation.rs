//
//  mutation.rs
//  Anchor
//
//  Created by hak (tharun)
//

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use super::engine::CodeGraph;
use super::types::*;

impl CodeGraph {
    /// Build the graph from a set of file extractions.
    pub fn build_from_extractions(&mut self, extractions: Vec<FileExtractions>) {
        debug!(
            file_count = extractions.len(),
            "ingesting extractions into graph"
        );
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

                self.add_edge(file_idx, sym_idx, EdgeKind::Defines);
            }

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

        // Phase 2: Resolve cross-references (calls) and collect call lines
        for extraction in &extractions {
            for call in &extraction.calls {
                let caller_key = (extraction.file_path.clone(), call.caller.clone());
                let callee_nodes = self.symbol_index.get(&call.callee).cloned();

                if let Some(&caller_idx) = self.qualified_index.get(&caller_key) {
                    if let Some(callee_indexes) = callee_nodes {
                        if let Some(&callee_idx) = callee_indexes.first() {
                            self.add_edge(caller_idx, callee_idx, EdgeKind::Calls);

                            if let Some(node) = self.graph.node_weight_mut(caller_idx) {
                                for line in call.line..=call.line_end {
                                    if !node.call_lines.contains(&line) {
                                        node.call_lines.push(line);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort call_lines for consistent slicing
        for idx in self.graph.node_indices() {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                node.call_lines.sort();
                node.call_lines.dedup();
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

    /// Find symbols whose line range overlaps [start, end] in a file.
    pub fn symbols_in_range(&self, file: &Path, start: usize, end: usize) -> Vec<&NodeData> {
        self.symbols_in_file(file)
            .into_iter()
            .filter(|s| s.line_start <= end && s.line_end >= start)
            .collect()
    }

    /// Incrementally update a file's symbols in the graph.
    /// Diffs old vs new symbols by name — only touches changed/added/removed nodes.
    /// Unchanged symbols keep their NodeIndex (stable graph references).
    pub fn update_file_incremental(
        &mut self,
        file: &Path,
        new_extraction: FileExtractions,
    ) {
        let file_idx = self.add_file(file.to_path_buf());

        // Collect old symbols: name -> (NodeIndex, code_snippet)
        let old_symbols: HashMap<String, (NodeIndex, String)> = self
            .graph
            .edges_directed(file_idx, petgraph::Direction::Outgoing)
            .filter(|e| {
                let kind = &e.weight().kind;
                *kind == EdgeKind::Defines && self.is_live(e.target())
            })
            .filter_map(|e| {
                let node = &self.graph[e.target()];
                if node.kind == NodeKind::Import {
                    None
                } else {
                    Some((node.name.clone(), (e.target(), node.code_snippet.clone())))
                }
            })
            .collect();

        let new_symbols: HashMap<String, &ExtractedSymbol> = new_extraction
            .symbols
            .iter()
            .map(|s| (s.name.clone(), s))
            .collect();

        let mut needs_call_resolution: Vec<NodeIndex> = Vec::new();

        // Removed symbols (in old, not in new)
        for (name, (node_idx, _)) in &old_symbols {
            if !new_symbols.contains_key(name) {
                if let Some(node) = self.graph.node_weight_mut(*node_idx) {
                    node.removed = true;
                    let file_path = node.file_path.clone();

                    if let Some(indexes) = self.symbol_index.get_mut(name) {
                        indexes.retain(|&idx| idx != *node_idx);
                        if indexes.is_empty() {
                            self.symbol_index.remove(name);
                        }
                    }
                    self.qualified_index.remove(&(file_path, name.clone()));
                }
            }
        }

        // Added symbols (in new, not in old)
        for (name, sym) in &new_symbols {
            if !old_symbols.contains_key(name) {
                let sym_idx = self.add_symbol(
                    sym.name.clone(),
                    sym.kind,
                    file.to_path_buf(),
                    sym.line_start,
                    sym.line_end,
                    sym.code_snippet.clone(),
                );
                self.add_edge(file_idx, sym_idx, EdgeKind::Defines);
                needs_call_resolution.push(sym_idx);
            }
        }

        // Changed symbols (same name, different code)
        for (name, sym) in &new_symbols {
            if let Some((node_idx, old_code)) = old_symbols.get(name) {
                if *old_code != sym.code_snippet {
                    if let Some(node) = self.graph.node_weight_mut(*node_idx) {
                        node.code_snippet = sym.code_snippet.clone();
                        node.line_start = sym.line_start;
                        node.line_end = sym.line_end;
                        node.call_lines.clear();
                    }

                    // Remove old outgoing Calls edges
                    let call_edges: Vec<petgraph::graph::EdgeIndex> = self
                        .graph
                        .edges_directed(*node_idx, petgraph::Direction::Outgoing)
                        .filter(|e| e.weight().kind == EdgeKind::Calls)
                        .map(|e| e.id())
                        .collect();
                    for eid in call_edges {
                        self.graph.remove_edge(eid);
                    }

                    needs_call_resolution.push(*node_idx);
                } else {
                    // Unchanged — just update line numbers if they shifted
                    if let Some(node) = self.graph.node_weight_mut(*node_idx) {
                        node.line_start = sym.line_start;
                        node.line_end = sym.line_end;
                    }
                }
            }
        }

        // Handle imports: remove old, add new
        let old_import_nodes: Vec<NodeIndex> = self
            .graph
            .edges_directed(file_idx, petgraph::Direction::Outgoing)
            .filter(|e| e.weight().kind == EdgeKind::Imports && self.is_live(e.target()))
            .map(|e| e.target())
            .collect();
        for &imp_idx in &old_import_nodes {
            if let Some(node) = self.graph.node_weight_mut(imp_idx) {
                let name = node.name.clone();
                let fp = node.file_path.clone();
                node.removed = true;
                if let Some(indexes) = self.symbol_index.get_mut(&name) {
                    indexes.retain(|&idx| idx != imp_idx);
                    if indexes.is_empty() {
                        self.symbol_index.remove(&name);
                    }
                }
                self.qualified_index.remove(&(fp, name));
            }
        }
        for import in &new_extraction.imports {
            let import_idx = self.add_symbol(
                import.path.clone(),
                NodeKind::Import,
                file.to_path_buf(),
                import.line,
                import.line,
                String::new(),
            );
            self.add_edge(file_idx, import_idx, EdgeKind::Imports);
        }

        // Re-resolve calls for changed/added symbols
        let nodes_needing_resolution: HashSet<NodeIndex> =
            needs_call_resolution.into_iter().collect();

        for call in &new_extraction.calls {
            let caller_key = (file.to_path_buf(), call.caller.clone());
            if let Some(&caller_idx) = self.qualified_index.get(&caller_key) {
                if !nodes_needing_resolution.contains(&caller_idx) {
                    continue;
                }
                if let Some(callee_indexes) = self.symbol_index.get(&call.callee).cloned() {
                    if let Some(&callee_idx) = callee_indexes.first() {
                        self.add_edge(caller_idx, callee_idx, EdgeKind::Calls);
                        if let Some(node) = self.graph.node_weight_mut(caller_idx) {
                            for line in call.line..=call.line_end {
                                if !node.call_lines.contains(&line) {
                                    node.call_lines.push(line);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort call_lines for changed nodes
        for &idx in &nodes_needing_resolution {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                node.call_lines.sort();
                node.call_lines.dedup();
            }
        }

        // Re-resolve contains for changed/added symbols
        for sym in &new_extraction.symbols {
            if let Some(ref parent_name) = sym.parent {
                let child_key = (file.to_path_buf(), sym.name.clone());
                let parent_key = (file.to_path_buf(), parent_name.clone());

                if let Some(&child_idx) = self.qualified_index.get(&child_key) {
                    if !nodes_needing_resolution.contains(&child_idx) {
                        continue;
                    }
                    if let Some(&parent_idx) = self.qualified_index.get(&parent_key) {
                        self.add_edge(parent_idx, child_idx, EdgeKind::Contains);
                    }
                }
            }
        }
    }

    /// Soft-delete all nodes originating from a specific file.
    pub fn remove_file(&mut self, path: &Path) {
        if let Some(&file_idx) = self.file_index.get(path) {
            debug!(file = %path.display(), "removing file from graph");
            let child_nodes: Vec<NodeIndex> = self
                .graph
                .edges_directed(file_idx, petgraph::Direction::Outgoing)
                .map(|e| e.target())
                .collect();

            for &node_idx in &child_nodes {
                if let Some(node) = self.graph.node_weight_mut(node_idx) {
                    let name = node.name.clone();
                    let file = node.file_path.clone();
                    node.removed = true;

                    if let Some(indexes) = self.symbol_index.get_mut(&name) {
                        indexes.retain(|&idx| idx != node_idx);
                        if indexes.is_empty() {
                            self.symbol_index.remove(&name);
                        }
                    }
                    self.qualified_index.remove(&(file, name));
                }
            }

            if let Some(file_node) = self.graph.node_weight_mut(file_idx) {
                file_node.removed = true;
            }
            self.file_index.remove(path);
        }
    }

    /// Rebuild the graph without soft-deleted nodes to reclaim memory.
    pub fn compact(&mut self) {
        info!("compacting graph — rebuilding without soft-deleted nodes");
        let mut live_files: HashMap<PathBuf, Vec<NodeIndex>> = HashMap::new();
        for idx in self.graph.node_indices() {
            let node = &self.graph[idx];
            if !node.removed && node.kind == NodeKind::File {
                live_files.insert(node.file_path.clone(), Vec::new());
            }
        }

        let mut new_graph = CodeGraph::new();

        for path in live_files.keys() {
            new_graph.add_file(path.clone());
        }

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

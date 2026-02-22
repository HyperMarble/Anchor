//
//  query.rs
//  Anchor
//
//  Created by hak (tharun)
//

use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{HashSet, VecDeque};
use std::path::Path;

use super::engine::CodeGraph;
use super::types::*;

impl CodeGraph {
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

        // If no exact match, fuzzy search by name + features
        if results.is_empty() {
            let mut scored: Vec<(usize, petgraph::graph::NodeIndex)> = Vec::new();
            let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

            for (name, indexes) in &self.symbol_index {
                let name_lower = name.to_lowercase();
                for &idx in indexes {
                    let node = &self.graph[idx];
                    if node.removed {
                        continue;
                    }

                    // Name-based scoring
                    if name_lower.contains(&query_lower) {
                        let score = if node.name == query {
                            0
                        } else if name_lower.starts_with(&query_lower) {
                            1
                        } else {
                            2
                        };
                        scored.push((score, idx));
                    } else if !node.features.is_empty() {
                        // Feature-based scoring: count how many query terms match features
                        let feature_matches = query_terms
                            .iter()
                            .filter(|t| t.len() > 2 && node.features.iter().any(|f| f.contains(*t)))
                            .count();
                        if feature_matches > 0 {
                            // Score 3 for single-term match, 2 for multi-term (better than substring)
                            let score = if feature_matches >= query_terms.len() {
                                3
                            } else {
                                4
                            };
                            scored.push((score, idx));
                        }
                    }
                }
            }

            scored.sort_by_key(|(score, _)| *score);

            for (_, idx) in scored.into_iter().take(limit) {
                if let Some(result) = self.build_search_result(idx) {
                    results.push(result);
                }
            }
        }

        results
    }

    /// Get all symbols in the graph (for regex filtering).
    pub fn all_symbols(&self) -> Vec<SearchResult> {
        self.symbol_index
            .values()
            .flatten()
            .filter_map(|&idx| self.build_search_result(idx))
            .collect()
    }

    /// Get all indexed file paths.
    pub fn all_files(&self) -> Vec<std::path::PathBuf> {
        self.file_index.keys().cloned().collect()
    }

    /// Graph-aware search: finds by file path OR symbol name, then traverses connections.
    ///
    /// 1. Try to match file paths (fuzzy)
    /// 2. Try to match symbol names
    /// 3. BFS traverse to get connected nodes
    pub fn search_graph(&self, query: &str, depth: usize) -> GraphSearchResult {
        const MAX_INITIAL_MATCHES: usize = 10;
        const MAX_SYMBOLS: usize = 50;
        const MAX_CONNECTIONS: usize = 100;

        let query_lower = query.to_lowercase();
        let mut result = GraphSearchResult::default();

        // 1. Try file path match first
        let file_matches: Vec<_> = self
            .file_index
            .iter()
            .filter(|(path, &idx)| {
                let path_str = path.to_string_lossy().to_lowercase();
                self.is_live(idx)
                    && (path_str.contains(&query_lower)
                        || path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase().contains(&query_lower))
                            .unwrap_or(false))
            })
            .take(MAX_INITIAL_MATCHES)
            .collect();

        if !file_matches.is_empty() {
            result.match_type = "file".to_string();

            let mut symbol_indexes: Vec<petgraph::graph::NodeIndex> = Vec::new();

            for (path, &file_idx) in &file_matches {
                if result.symbols.len() >= MAX_SYMBOLS {
                    break;
                }
                result.matched_files.push(path.to_path_buf());

                for edge in self.graph.edges_directed(file_idx, Direction::Outgoing) {
                    if result.symbols.len() >= MAX_SYMBOLS {
                        break;
                    }
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

            // Traverse connections if depth > 0
            if depth > 0 {
                let mut visited: HashSet<petgraph::graph::NodeIndex> =
                    symbol_indexes.iter().copied().collect();

                for &idx in &symbol_indexes {
                    if result.connections.len() >= MAX_CONNECTIONS {
                        break;
                    }
                    let node = &self.graph[idx];

                    for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                        if result.connections.len() >= MAX_CONNECTIONS {
                            break;
                        }
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

                    for edge in self.graph.edges_directed(idx, Direction::Incoming) {
                        if result.connections.len() >= MAX_CONNECTIONS {
                            break;
                        }
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

            if result.symbols.len() >= MAX_SYMBOLS || result.connections.len() >= MAX_CONNECTIONS {
                result.truncated = true;
            }

            return result;
        }

        // 2. Try symbol name match — exact or prefix only
        let symbol_matches: Vec<petgraph::graph::NodeIndex> = self
            .symbol_index
            .iter()
            .filter(|(name, _)| {
                let name_lower = name.to_lowercase();
                name_lower == query_lower || name_lower.starts_with(&query_lower)
            })
            .flat_map(|(_, indexes)| indexes.iter().copied())
            .filter(|&idx| self.is_live(idx))
            .take(MAX_INITIAL_MATCHES)
            .collect();

        if symbol_matches.is_empty() {
            result.match_type = "none".to_string();
            return result;
        }

        result.match_type = "symbol".to_string();

        // 3. BFS traverse from matched symbols
        let mut visited: HashSet<petgraph::graph::NodeIndex> = HashSet::new();
        let mut queue: VecDeque<(petgraph::graph::NodeIndex, usize)> = VecDeque::new();

        for idx in &symbol_matches {
            queue.push_back((*idx, 0));
            visited.insert(*idx);
        }

        while let Some((idx, current_depth)) = queue.pop_front() {
            if result.symbols.len() >= MAX_SYMBOLS && result.connections.len() >= MAX_CONNECTIONS {
                break;
            }

            let node = &self.graph[idx];

            if node.kind != NodeKind::File && result.symbols.len() < MAX_SYMBOLS {
                result.symbols.push(SymbolInfo {
                    name: node.name.clone(),
                    kind: node.kind,
                    file: node.file_path.clone(),
                    line: node.line_start,
                    code: node.code_snippet.clone(),
                });
            }

            if current_depth < depth && result.connections.len() < MAX_CONNECTIONS {
                for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                    if result.connections.len() >= MAX_CONNECTIONS {
                        break;
                    }
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

                for edge in self.graph.edges_directed(idx, Direction::Incoming) {
                    if result.connections.len() >= MAX_CONNECTIONS {
                        break;
                    }
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

        if result.symbols.len() >= MAX_SYMBOLS || result.connections.len() >= MAX_CONNECTIONS {
            result.truncated = true;
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
                .filter(|edge| {
                    edge.weight().kind == EdgeKind::Defines && self.is_live(edge.target())
                })
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
                if node.removed {
                    None
                } else {
                    Some(node)
                }
            })
    }

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

    /// Build a SearchResult from a node index, including connections.
    pub(crate) fn build_search_result(
        &self,
        idx: petgraph::graph::NodeIndex,
    ) -> Option<SearchResult> {
        let node = &self.graph[idx];

        if node.kind == NodeKind::File || node.removed {
            return None;
        }

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
            call_lines: node.call_lines.clone(),
            calls,
            called_by,
            imports,
            features: node.features.clone(),
        })
    }
}

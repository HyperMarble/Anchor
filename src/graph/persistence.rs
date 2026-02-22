//
//  persistence.rs
//  Anchor
//
//  Created by hak (tharun)
//

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;
use tracing::{debug, info};

use super::engine::CodeGraph;
use super::types::{EdgeData, NodeData, NodeKind};
use crate::error::{AnchorError, Result};

/// Serializable representation of the graph.
/// Nodes are stored as a flat vec; edges reference nodes by index position.
#[derive(Serialize, Deserialize)]
struct SerializableGraph {
    nodes: Vec<NodeData>,
    edges: Vec<(u32, u32, EdgeData)>,
}

impl CodeGraph {
    /// Save the graph to a binary file.
    ///
    /// Uses atomic write: writes to a `.tmp` file first, then renames.
    /// This prevents corruption if the process is interrupted mid-write.
    pub fn save(&self, path: &Path) -> Result<()> {
        info!(path = %path.display(), "saving graph");

        let sg = self.to_serializable();
        let bytes =
            bincode::serialize(&sg).map_err(|e| AnchorError::SerializeError(e.to_string()))?;

        // Atomic write: write to .tmp, then rename
        let tmp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&tmp_path, path)?;

        debug!(bytes = bytes.len(), "graph saved");
        Ok(())
    }

    /// Load a graph from a binary file.
    pub fn load(path: &Path) -> Result<Self> {
        info!(path = %path.display(), "loading graph");

        let bytes = fs::read(path)?;
        let sg: SerializableGraph = bincode::deserialize(&bytes)
            .map_err(|e| AnchorError::ParseError(format!("bincode: {}", e)))?;

        let graph = Self::from_serializable(sg);

        let stats = graph.stats();
        debug!(
            files = stats.file_count,
            symbols = stats.symbol_count,
            edges = stats.total_edges,
            "graph loaded"
        );

        Ok(graph)
    }

    /// Convert to a serializable representation.
    fn to_serializable(&self) -> SerializableGraph {
        let graph = self.inner_graph();

        // Collect nodes in index order
        let nodes: Vec<NodeData> = graph.node_indices().map(|idx| graph[idx].clone()).collect();

        // Collect edges as (source_index, target_index, data)
        let edges: Vec<(u32, u32, EdgeData)> = graph
            .edge_indices()
            .filter_map(|eidx| {
                graph
                    .edge_endpoints(eidx)
                    .map(|(src, tgt)| (src.index() as u32, tgt.index() as u32, graph[eidx].clone()))
            })
            .collect();

        SerializableGraph { nodes, edges }
    }

    /// Reconstruct from a serializable representation.
    fn from_serializable(sg: SerializableGraph) -> Self {
        use petgraph::graph::NodeIndex;

        let mut graph = Self::new();

        // Add all nodes
        let mut index_map: Vec<NodeIndex> = Vec::with_capacity(sg.nodes.len());
        for node in sg.nodes {
            let idx = if node.kind == NodeKind::File {
                graph.add_file(node.file_path.clone())
            } else {
                graph.add_symbol(
                    node.name.clone(),
                    node.kind,
                    node.file_path.clone(),
                    node.line_start,
                    node.line_end,
                    node.code_snippet.clone(),
                )
            };

            // Restore metadata that add_symbol/add_file doesn't set
            if let Some(n) = graph.inner_graph_mut().node_weight_mut(idx) {
                if node.removed {
                    n.removed = true;
                }
                if !node.call_lines.is_empty() {
                    n.call_lines = node.call_lines;
                }
                if !node.features.is_empty() {
                    n.features = node.features;
                }
            }

            index_map.push(idx);
        }

        // Add all edges
        for (src, tgt, data) in sg.edges {
            let src_idx = index_map[src as usize];
            let tgt_idx = index_map[tgt as usize];
            graph.add_edge(src_idx, tgt_idx, data.kind);
        }

        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load_roundtrip() {
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/main.rs"));
        let fn_idx = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1,
            10,
            "fn main() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        let helper_idx = graph.add_symbol(
            "helper".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            12,
            20,
            "fn helper() {}".to_string(),
        );
        graph.add_edge(file_idx, helper_idx, EdgeKind::Defines);
        graph.add_edge(fn_idx, helper_idx, EdgeKind::Calls);

        let dir = tempdir().unwrap();
        let save_path = dir.path().join("graph.bin");

        // Save
        graph.save(&save_path).unwrap();
        assert!(save_path.exists());

        // Load
        let loaded = CodeGraph::load(&save_path).unwrap();

        // Verify stats match
        let orig_stats = graph.stats();
        let loaded_stats = loaded.stats();
        assert_eq!(orig_stats.file_count, loaded_stats.file_count);
        assert_eq!(orig_stats.symbol_count, loaded_stats.symbol_count);
        assert_eq!(orig_stats.total_edges, loaded_stats.total_edges);

        // Verify search works
        let results = loaded.search("main", 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol, "main");
        assert_eq!(results[0].calls.len(), 1);
        assert_eq!(results[0].calls[0].name, "helper");
    }

    #[test]
    fn test_save_load_preserves_removed_nodes() {
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/old.rs"));
        let fn_idx = graph.add_symbol(
            "old_fn".to_string(),
            NodeKind::Function,
            PathBuf::from("src/old.rs"),
            1,
            5,
            "fn old_fn() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        // Remove the file (soft-delete)
        graph.remove_file(std::path::Path::new("src/old.rs"));

        let dir = tempdir().unwrap();
        let save_path = dir.path().join("graph.bin");

        graph.save(&save_path).unwrap();
        let loaded = CodeGraph::load(&save_path).unwrap();

        // Removed nodes should still be invisible after load
        let stats = loaded.stats();
        assert_eq!(stats.file_count, 0);
        assert_eq!(stats.symbol_count, 0);
        assert_eq!(loaded.search("old_fn", 3).len(), 0);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = CodeGraph::load(Path::new("/nonexistent/graph.bin"));
        assert!(result.is_err());
    }
}

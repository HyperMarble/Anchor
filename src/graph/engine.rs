//
//  engine.rs
//  Anchor
//
//  Created by hak (tharun)
//

use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::path::PathBuf;

use super::types::*;

/// The main code graph — holds all nodes, edges, and indexes for fast lookup.
#[derive(Clone)]
pub struct CodeGraph {
    /// The directed graph storing code relationships.
    pub(crate) graph: DiGraph<NodeData, EdgeData>,
    /// Index: file path -> node index (for File nodes).
    pub(crate) file_index: HashMap<PathBuf, NodeIndex>,
    /// Index: symbol name -> list of node indexes (for quick name lookup).
    pub(crate) symbol_index: HashMap<String, Vec<NodeIndex>>,
    /// Index: (file_path, symbol_name) -> node index (for unique symbol resolution).
    pub(crate) qualified_index: HashMap<(PathBuf, String), NodeIndex>,
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

        self.symbol_index.entry(name.clone()).or_default().push(idx);
        self.qualified_index.insert((file_path, name), idx);

        idx
    }

    // ─── Edge Operations ────────────────────────────────────────

    /// Add an edge between two nodes.
    pub fn add_edge(&mut self, from: NodeIndex, to: NodeIndex, kind: EdgeKind) {
        self.graph.add_edge(from, to, EdgeData::new(kind));
    }

    // ─── Internal Helpers ───────────────────────────────────────

    /// Check if a node is live (not removed).
    pub(crate) fn is_live(&self, idx: NodeIndex) -> bool {
        self.graph.node_weight(idx).is_some_and(|n| !n.removed)
    }
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

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
                    features: vec![],
                },
                ExtractedSymbol {
                    name: "multiply".to_string(),
                    kind: NodeKind::Function,
                    line_start: 5,
                    line_end: 7,
                    code_snippet: "fn multiply(a: i32, b: i32) -> i32 { a * b }".to_string(),
                    parent: None,
                    features: vec![],
                },
            ],
            imports: vec![],
            calls: vec![ExtractedCall {
                caller: "multiply".to_string(),
                callee: "add".to_string(),
                line: 6,
                line_end: 6,
            }],
        }];

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(extractions);

        let stats = graph.stats();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.symbol_count, 2);

        let results = graph.search("multiply", 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].calls.len(), 1);
        assert_eq!(results[0].calls[0].name, "add");

        assert_eq!(
            results[0].call_lines,
            vec![6],
            "call_lines should contain line 6 where multiply calls add"
        );
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
            1,
            10,
            "fn login() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        assert_eq!(graph.stats().file_count, 1);
        assert_eq!(graph.stats().symbol_count, 1);

        graph.remove_file(Path::new("src/auth.rs"));

        let stats = graph.stats();
        assert_eq!(stats.file_count, 0, "File should be removed from stats");
        assert_eq!(
            stats.symbol_count, 0,
            "Symbols should be removed from stats"
        );
    }

    #[test]
    fn test_remove_file_hides_from_search() {
        let mut graph = CodeGraph::new();
        let file_idx = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn_idx = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1,
            10,
            "fn login() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        assert_eq!(graph.search("login", 3).len(), 1);

        graph.remove_file(Path::new("src/auth.rs"));

        assert_eq!(
            graph.search("login", 3).len(),
            0,
            "Removed symbol should not appear in search"
        );
    }

    #[test]
    fn test_remove_and_readd_no_duplicates() {
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn_idx = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1,
            10,
            "fn login() { v1 }".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        graph.remove_file(Path::new("src/auth.rs"));

        let file_idx2 = graph.add_file(PathBuf::from("src/auth.rs"));
        let fn_idx2 = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1,
            15,
            "fn login() { v2 }".to_string(),
        );
        graph.add_edge(file_idx2, fn_idx2, EdgeKind::Defines);

        let results = graph.search("login", 3);
        assert_eq!(
            results.len(),
            1,
            "Should have exactly 1 result after re-add"
        );
        assert!(results[0].code.contains("v2"), "Should have updated code");

        let stats = graph.stats();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.symbol_count, 1);
    }

    #[test]
    fn test_remove_file_preserves_other_files() {
        let mut graph = CodeGraph::new();

        let file_a = graph.add_file(PathBuf::from("src/auth.rs"));
        let login = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1,
            10,
            "fn login() {}".to_string(),
        );
        graph.add_edge(file_a, login, EdgeKind::Defines);

        let file_b = graph.add_file(PathBuf::from("src/main.rs"));
        let main_fn = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1,
            5,
            "fn main() {}".to_string(),
        );
        graph.add_edge(file_b, main_fn, EdgeKind::Defines);

        graph.remove_file(Path::new("src/auth.rs"));

        let results = graph.search("main", 3);
        assert_eq!(results.len(), 1, "File B should be unaffected");
        assert_eq!(results[0].symbol, "main");

        assert_eq!(graph.search("login", 3).len(), 0);

        let stats = graph.stats();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.symbol_count, 1);
    }

    #[test]
    fn test_remove_file_clears_cross_references() {
        let mut graph = CodeGraph::new();

        let file_a = graph.add_file(PathBuf::from("src/main.rs"));
        let main_fn = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1,
            10,
            "fn main() { login(); }".to_string(),
        );
        graph.add_edge(file_a, main_fn, EdgeKind::Defines);

        let file_b = graph.add_file(PathBuf::from("src/auth.rs"));
        let login_fn = graph.add_symbol(
            "login".to_string(),
            NodeKind::Function,
            PathBuf::from("src/auth.rs"),
            1,
            10,
            "fn login() {}".to_string(),
        );
        graph.add_edge(file_b, login_fn, EdgeKind::Defines);
        graph.add_edge(main_fn, login_fn, EdgeKind::Calls);

        let results = graph.search("login", 3);
        assert_eq!(results[0].called_by.len(), 1);

        graph.remove_file(Path::new("src/main.rs"));

        let results = graph.search("login", 3);
        assert_eq!(results.len(), 1, "login itself should still exist");
        assert_eq!(
            results[0].called_by.len(),
            0,
            "Removed caller should disappear from called_by"
        );
    }

    #[test]
    fn test_compact_reclaims_memory() {
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/old.rs"));
        let fn_idx = graph.add_symbol(
            "old_fn".to_string(),
            NodeKind::Function,
            PathBuf::from("src/old.rs"),
            1,
            10,
            "fn old_fn() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

        let file_keep = graph.add_file(PathBuf::from("src/keep.rs"));
        let keep_fn = graph.add_symbol(
            "keep_fn".to_string(),
            NodeKind::Function,
            PathBuf::from("src/keep.rs"),
            1,
            5,
            "fn keep_fn() {}".to_string(),
        );
        graph.add_edge(file_keep, keep_fn, EdgeKind::Defines);

        graph.remove_file(Path::new("src/old.rs"));

        let stats_before = graph.stats();
        assert_eq!(stats_before.file_count, 1);
        assert_eq!(stats_before.symbol_count, 1);

        graph.compact();

        let stats_after = graph.stats();
        assert_eq!(stats_after.file_count, 1);
        assert_eq!(stats_after.symbol_count, 1);

        let results = graph.search("keep_fn", 3);
        assert_eq!(results.len(), 1);

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
            1,
            5,
            "fn main() {}".to_string(),
        );
        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);

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
        let mut graph = CodeGraph::new();

        let file_idx = graph.add_file(PathBuf::from("src/cycle.rs"));
        let a_idx = graph.add_symbol(
            "func_a".to_string(),
            NodeKind::Function,
            PathBuf::from("src/cycle.rs"),
            1,
            5,
            "fn func_a() { func_b(); }".to_string(),
        );
        let b_idx = graph.add_symbol(
            "func_b".to_string(),
            NodeKind::Function,
            PathBuf::from("src/cycle.rs"),
            6,
            10,
            "fn func_b() { func_a(); }".to_string(),
        );

        graph.add_edge(file_idx, a_idx, EdgeKind::Defines);
        graph.add_edge(file_idx, b_idx, EdgeKind::Defines);
        graph.add_edge(a_idx, b_idx, EdgeKind::Calls);
        graph.add_edge(b_idx, a_idx, EdgeKind::Calls);

        let results = graph.search("func_a", 3);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].calls.len(), 1);
        assert_eq!(results[0].called_by.len(), 1);

        let deps = graph.dependencies("func_a");
        assert!(!deps.is_empty());
        let dependents = graph.dependents("func_a");
        assert!(!dependents.is_empty());
    }

    #[test]
    fn test_duplicate_symbol_names_across_files() {
        let mut graph = CodeGraph::new();

        let file_a = graph.add_file(PathBuf::from("src/a.rs"));
        let init_a = graph.add_symbol(
            "init".to_string(),
            NodeKind::Function,
            PathBuf::from("src/a.rs"),
            1,
            5,
            "fn init() { /* a */ }".to_string(),
        );
        graph.add_edge(file_a, init_a, EdgeKind::Defines);

        let file_b = graph.add_file(PathBuf::from("src/b.rs"));
        let init_b = graph.add_symbol(
            "init".to_string(),
            NodeKind::Function,
            PathBuf::from("src/b.rs"),
            1,
            5,
            "fn init() { /* b */ }".to_string(),
        );
        graph.add_edge(file_b, init_b, EdgeKind::Defines);

        let results = graph.search("init", 10);
        assert_eq!(results.len(), 2);

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
        graph.remove_file(Path::new("does_not_exist.rs"));
        assert_eq!(graph.stats().total_nodes, 0);
    }

    #[test]
    fn test_stats_edge_count_excludes_nothing() {
        let mut graph = CodeGraph::new();
        let file_idx = graph.add_file(PathBuf::from("src/main.rs"));
        let fn_idx = graph.add_symbol(
            "main".to_string(),
            NodeKind::Function,
            PathBuf::from("src/main.rs"),
            1,
            5,
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
            1,
            5,
            "fn process() {}".to_string(),
        );

        graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);
        graph.add_edge(file_idx, fn_idx, EdgeKind::Contains);

        assert_eq!(graph.stats().total_edges, 2);
    }

    #[test]
    fn test_write_then_rebuild_updates_line_numbers() {
        use std::io::Write;

        // Create a temp file with two functions
        let dir = tempfile::tempdir().unwrap();
        let test_file = dir.path().join("test.rs");
        {
            let mut f = std::fs::File::create(&test_file).unwrap();
            write!(f, "fn foo() {{\n    let x = 1;\n}}\n\nfn bar() {{\n    let y = 2;\n}}\n").unwrap();
        }

        // Build graph
        let source = std::fs::read_to_string(&test_file).unwrap();
        let extraction = crate::parser::extractor::extract_file(&test_file, &source).unwrap();
        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        // bar starts at line 5
        let results = graph.search("bar", 5);
        assert!(!results.is_empty(), "bar should be found");
        assert_eq!(results[0].line_start, 5);

        // Write: add 2 lines inside foo (lines shift)
        crate::write::replace_range(&test_file, 2, 2, "    let x = 1;\n    let z = 3;\n    let w = 4;").unwrap();

        // Before rebuild: graph still says bar is at line 5 (stale)
        let results = graph.search("bar", 5);
        assert_eq!(results[0].line_start, 5, "should be stale before rebuild");

        // After rebuild: bar should be at line 7
        crate::graph::rebuild_file(&mut graph, &test_file).unwrap();
        let results = graph.search("bar", 5);
        assert_eq!(results[0].line_start, 7, "rebuild should update line numbers");
    }
}

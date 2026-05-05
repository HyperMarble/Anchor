use anchor::graph::types::{EdgeKind, NodeKind};
use anchor::graph::CodeGraph;
use std::path::PathBuf;

#[test]
fn test_empty_graph_has_no_symbols() {
    let graph = CodeGraph::new();
    let stats = graph.stats();
    assert_eq!(stats.total_nodes, 0);
    assert_eq!(stats.total_edges, 0);
}

#[test]
fn test_search_returns_matching_symbol() {
    let mut graph = CodeGraph::new();
    let file_idx = graph.add_file(PathBuf::from("src/auth.rs"));
    let fn_idx = graph.add_symbol(
        "authenticate".to_string(),
        NodeKind::Function,
        PathBuf::from("src/auth.rs"),
        1,
        10,
        "pub fn authenticate() {}".to_string(),
    );
    graph.add_edge(file_idx, fn_idx, EdgeKind::Defines);
    let results = graph.search("authenticate", 1);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].symbol, "authenticate");
}

#[test]
fn test_search_returns_empty_for_unknown_symbol() {
    let graph = CodeGraph::new();
    let results = graph.search("nonexistent", 1);
    assert!(results.is_empty());
}

#[test]
fn test_multiple_symbols_in_file() {
    let mut graph = CodeGraph::new();
    let file_idx = graph.add_file(PathBuf::from("src/lib.rs"));
    for name in &["foo", "bar", "baz"] {
        let sym = graph.add_symbol(
            name.to_string(),
            NodeKind::Function,
            PathBuf::from("src/lib.rs"),
            1,
            5,
            format!("fn {}() {{}}", name),
        );
        graph.add_edge(file_idx, sym, EdgeKind::Defines);
    }
    assert_eq!(graph.stats().symbol_count, 3);
}

#[test]
fn test_add_edge_increases_edge_count() {
    let mut graph = CodeGraph::new();
    let f = graph.add_file(PathBuf::from("a.rs"));
    let s = graph.add_symbol(
        "myfn".to_string(),
        NodeKind::Function,
        PathBuf::from("a.rs"),
        1,
        1,
        "fn myfn() {}".to_string(),
    );
    graph.add_edge(f, s, EdgeKind::Defines);
    assert_eq!(graph.stats().total_edges, 1);
}

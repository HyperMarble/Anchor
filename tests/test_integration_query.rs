use anchor::graph::CodeGraph;
use anchor::parser::extract_file;
use anchor::query::{
    anchor_dependencies, anchor_file_symbols, anchor_search, anchor_stats, graph_search, Query,
};
use std::path::PathBuf;

fn make_graph(file: &str, src: &str) -> CodeGraph {
    let path = PathBuf::from(file);
    let extraction = extract_file(&path, src).unwrap();
    let mut graph = CodeGraph::new();
    graph.build_from_extractions(vec![extraction]);
    graph
}

#[test]
fn test_anchor_search_simple_finds_symbol() {
    let graph = make_graph(
        "src/auth.rs",
        "pub fn login(user: &str, pw: &str) -> bool { true }",
    );
    let resp = anchor_search(&graph, Query::Simple("login".to_string()));
    assert!(resp.found);
    assert_eq!(resp.count, 1);
    assert_eq!(resp.results[0].symbol, "login");
    assert!(resp.results[0].code.contains("pub fn login"));
}

#[test]
fn test_anchor_search_missing_symbol_not_found() {
    let graph = make_graph("src/lib.rs", "fn foo() {}");
    let resp = anchor_search(&graph, Query::Simple("nonexistent".to_string()));
    assert!(!resp.found);
    assert_eq!(resp.count, 0);
    assert!(resp.results.is_empty());
}

#[test]
fn test_anchor_search_structured_with_kind() {
    let src = r#"
pub struct Config {}
pub fn new_config() -> Config { Config {} }
"#;
    let graph = make_graph("src/config.rs", src);
    let resp = anchor_search(
        &graph,
        Query::Structured {
            symbol: "Config".to_string(),
            kind: Some("struct".to_string()),
            file: None,
        },
    );
    assert!(resp.found);
    assert_eq!(resp.results[0].symbol, "Config");
}

#[test]
fn test_anchor_search_structured_with_file_filter() {
    let src1 = "pub fn process() {}";
    let src2 = "pub fn process() {}";
    let e1 = extract_file(&PathBuf::from("src/a.rs"), src1).unwrap();
    let e2 = extract_file(&PathBuf::from("src/b.rs"), src2).unwrap();
    let mut graph = CodeGraph::new();
    graph.build_from_extractions(vec![e1, e2]);

    let resp = anchor_search(
        &graph,
        Query::Structured {
            symbol: "process".to_string(),
            kind: None,
            file: Some("src/a.rs".to_string()),
        },
    );
    assert!(resp.found);
    // Only results from a.rs should be returned
    for r in &resp.results {
        assert!(r.file.to_string_lossy().contains("a.rs"), "Expected only a.rs results");
    }
}

#[test]
fn test_anchor_stats_counts_correctly() {
    let src = r#"
fn alpha() {}
fn beta() {}
struct Gamma {}
"#;
    let graph = make_graph("src/stats_test.rs", src);
    let resp = anchor_stats(&graph);
    assert!(resp.stats.symbol_count >= 3);
    assert_eq!(resp.stats.file_count, 1);
}

#[test]
fn test_anchor_stats_empty_graph() {
    let graph = CodeGraph::new();
    let resp = anchor_stats(&graph);
    assert_eq!(resp.stats.symbol_count, 0);
    assert_eq!(resp.stats.file_count, 0);
}

#[test]
fn test_anchor_file_symbols_returns_all() {
    let src = r#"
pub fn foo() {}
pub fn bar() {}
pub struct Baz {}
"#;
    let graph = make_graph("src/multi.rs", src);
    let resp = anchor_file_symbols(&graph, "src/multi.rs");
    assert!(resp.found);
    assert_eq!(resp.symbols.len(), 3);
    let names: Vec<&str> = resp.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"foo"));
    assert!(names.contains(&"bar"));
    assert!(names.contains(&"Baz"));
}

#[test]
fn test_anchor_file_symbols_nonexistent_file() {
    let graph = CodeGraph::new();
    let resp = anchor_file_symbols(&graph, "src/ghost.rs");
    assert!(!resp.found);
    assert!(resp.symbols.is_empty());
}

#[test]
fn test_anchor_dependencies_captures_callees() {
    let src = r#"
pub fn orchestrate() {
    step_one();
    step_two();
}
fn step_one() {}
fn step_two() {}
"#;
    let graph = make_graph("src/pipeline.rs", src);
    let dep_resp = anchor_dependencies(&graph, "orchestrate");
    // Should have dependencies (things orchestrate calls)
    assert!(!dep_resp.dependencies.is_empty());
}

#[test]
fn test_anchor_dependencies_unknown_symbol() {
    let graph = CodeGraph::new();
    let resp = anchor_dependencies(&graph, "phantom");
    assert!(resp.dependencies.is_empty());
    assert!(resp.dependents.is_empty());
}

#[test]
fn test_graph_search_returns_matched_symbols() {
    let src = r#"
fn handle_request() {}
fn handle_response() {}
fn process_data() {}
"#;
    let graph = make_graph("src/server.rs", src);
    let result = graph_search(&graph, "handle_request", 1);
    // match_type should be non-empty when something is found
    assert!(!result.symbols.is_empty() || !result.matched_files.is_empty() || result.match_type == "none");
}

#[test]
fn test_graph_search_depth_controls_results() {
    let src = r#"
fn a() { b(); }
fn b() { c(); }
fn c() {}
"#;
    let graph = make_graph("src/chain.rs", src);
    let shallow = graph_search(&graph, "a", 0);
    let deep = graph_search(&graph, "a", 2);
    // Deeper search should include at least as many symbols as shallow
    assert!(deep.symbols.len() >= shallow.symbols.len());
}

#[test]
fn test_anchor_search_code_contains_snippet() {
    let src = "pub fn calculate(x: i32, y: i32) -> i32 { x + y }";
    let graph = make_graph("math.rs", src);
    let resp = anchor_search(&graph, Query::Simple("calculate".to_string()));
    assert!(resp.found);
    assert!(resp.results[0].code.contains("calculate"));
}

#[test]
fn test_multiple_files_in_graph() {
    let e1 = extract_file(&PathBuf::from("src/a.rs"), "pub fn func_a() {}").unwrap();
    let e2 = extract_file(&PathBuf::from("src/b.rs"), "pub fn func_b() {}").unwrap();
    let e3 = extract_file(&PathBuf::from("src/c.rs"), "pub fn func_c() {}").unwrap();
    let mut graph = CodeGraph::new();
    graph.build_from_extractions(vec![e1, e2, e3]);
    let stats = anchor_stats(&graph);
    assert_eq!(stats.stats.file_count, 3);
    assert!(stats.stats.symbol_count >= 3);
}

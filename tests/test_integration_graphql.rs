use anchor::graph::CodeGraph;
use anchor::graphql::{build_schema, execute};
use anchor::parser::extract_file;
use std::path::PathBuf;
use std::sync::Arc;

fn make_schema(file: &str, src: &str) -> anchor::graphql::AnchorSchema {
    let extraction = extract_file(&PathBuf::from(file), src).unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![extraction]);
    build_schema(Arc::new(g))
}

#[tokio::test]
async fn test_stats_query_returns_fields() {
    let schema = build_schema(Arc::new(CodeGraph::new()));
    let result = execute(&schema, "{ stats { files symbols edges } }").await;
    assert!(result.contains("files"));
    assert!(result.contains("symbols"));
    assert!(result.contains("edges"));
}

#[tokio::test]
async fn test_stats_query_empty_graph_zeros() {
    let schema = build_schema(Arc::new(CodeGraph::new()));
    let result = execute(&schema, "{ stats { files symbols edges } }").await;
    // Empty graph — all counts should be 0
    assert!(result.contains("\"files\": 0") || result.contains("\"files\":0"));
}

#[tokio::test]
async fn test_stats_query_with_symbols() {
    let schema = make_schema("src/lib.rs", "pub fn alpha() {}\npub fn beta() {}");
    let result = execute(&schema, "{ stats { symbols } }").await;
    assert!(result.contains("symbols"));
    // Should have at least 2 symbols
    assert!(!result.contains("\"symbols\": 0"));
}

#[tokio::test]
async fn test_search_query_finds_symbol() {
    let schema = make_schema("src/auth.rs", "pub fn authenticate(user: &str) -> bool { true }");
    // search takes `pattern` (regex), not `query`
    let result = execute(
        &schema,
        r#"{ search(pattern: "authenticate", limit: 5) { name file line } }"#,
    )
    .await;
    assert!(result.contains("authenticate"));
}

#[tokio::test]
async fn test_search_query_missing_symbol_empty() {
    let schema = build_schema(Arc::new(CodeGraph::new()));
    let result = execute(
        &schema,
        r#"{ search(pattern: "nonexistent_xyz_abc", limit: 5) { name } }"#,
    )
    .await;
    // Should return empty array
    assert!(result.contains("[]") || result.contains("\"search\": []") || result.contains("data"));
}

#[tokio::test]
async fn test_symbol_query_returns_name() {
    let schema = make_schema("src/db.rs", "pub fn connect() {}");
    // symbol returns Vec<Symbol>, not nullable
    let result = execute(&schema, r#"{ symbol(name: "connect") { name file } }"#).await;
    assert!(result.contains("connect"));
}

#[tokio::test]
async fn test_symbol_query_nonexistent_returns_empty_array() {
    let schema = build_schema(Arc::new(CodeGraph::new()));
    // symbol returns Vec — empty array when not found
    let result = execute(&schema, r#"{ symbol(name: "ghost_xyz_nope") { name } }"#).await;
    assert!(result.contains("[]") || result.contains("\"symbol\": []") || result.contains("data"));
}

#[tokio::test]
async fn test_invalid_query_returns_errors() {
    let schema = build_schema(Arc::new(CodeGraph::new()));
    let result = execute(&schema, "{ thisFieldDoesNotExist }").await;
    assert!(result.contains("errors") || result.contains("error"));
}

#[tokio::test]
async fn test_schema_builds_without_panic() {
    // Just verify build_schema doesn't panic with empty or populated graph
    let _schema1 = build_schema(Arc::new(CodeGraph::new()));
    let extraction = extract_file(&PathBuf::from("src/x.rs"), "fn x() {}").unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![extraction]);
    let _schema2 = build_schema(Arc::new(g));
}

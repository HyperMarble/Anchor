//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod mutation;
pub mod query;
pub mod schema;

use async_graphql::{EmptySubscription, Schema};
use std::sync::Arc;

use crate::graph::CodeGraph;
use mutation::Mutation;
use query::Query;

/// The Anchor GraphQL schema type
pub type AnchorSchema = Schema<Query, Mutation, EmptySubscription>;

/// Build the GraphQL schema with the code graph as context
pub fn build_schema(graph: Arc<CodeGraph>) -> AnchorSchema {
    Schema::build(Query, Mutation, EmptySubscription)
        .data(graph)
        .limit_depth(5) // Prevent infinite nesting
        .limit_complexity(100) // Prevent overly complex queries
        .finish()
}

/// Execute a GraphQL query and return JSON result
pub async fn execute(schema: &AnchorSchema, query: &str) -> String {
    let result = schema.execute(query).await;
    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::CodeGraph;

    #[tokio::test]
    async fn test_stats_query() {
        let graph = Arc::new(CodeGraph::new());
        let schema = build_schema(graph);

        let result = execute(&schema, "{ stats { files symbols edges } }").await;

        assert!(result.contains("files"));
        assert!(result.contains("symbols"));
        assert!(result.contains("edges"));
    }

    #[tokio::test]
    async fn test_symbol_query_empty() {
        let graph = Arc::new(CodeGraph::new());
        let schema = build_schema(graph);

        let result = execute(&schema, r#"{ symbol(name: "nonexistent") { name file } }"#).await;

        // Should return empty array, no errors
        assert!(result.contains("symbol"));
        assert!(!result.contains("error"));
    }

    #[tokio::test]
    async fn test_graph_slicing_through_graphql() {
        use crate::graph::types::*;
        use std::path::PathBuf;

        let mut graph = CodeGraph::new();
        // Build a long function (>10 lines) that calls another function
        let long_code = "fn caller() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n    let e = 5;\n    let f = 6;\n    let g = 7;\n    let h = 8;\n    let i = 9;\n    let result = callee();\n    let j = 10;\n    result\n}";
        graph.build_from_extractions(vec![FileExtractions {
            file_path: PathBuf::from("test.rs"),
            symbols: vec![
                ExtractedSymbol {
                    name: "caller".to_string(),
                    kind: NodeKind::Function,
                    line_start: 1,
                    line_end: 14,
                    code_snippet: long_code.to_string(),
                    parent: None,
                    features: vec![],
                },
                ExtractedSymbol {
                    name: "callee".to_string(),
                    kind: NodeKind::Function,
                    line_start: 20,
                    line_end: 22,
                    code_snippet: "fn callee() -> i32 { 42 }".to_string(),
                    parent: None,
                    features: vec![],
                },
            ],
            imports: vec![],
            calls: vec![ExtractedCall {
                caller: "caller".to_string(),
                callee: "callee".to_string(),
                line: 11,
                line_end: 11,
            }],
        }]);

        let schema = build_schema(Arc::new(graph));
        let result = execute(&schema, r#"{ symbol(name: "caller", exact: true) { name code } }"#).await;

        eprintln!("GraphQL result: {}", result);

        // Sliced code should contain "..." and line numbers
        assert!(result.contains("callee()"), "should contain the call to callee");
        assert!(result.contains("..."), "should have ... for skipped sections");
        assert!(result.contains("fn caller()"), "should have the signature");
    }
}

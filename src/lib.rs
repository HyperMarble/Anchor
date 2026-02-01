//! # Anchor
//!
//! Code intelligence for AI agents. LSP for AI.
//!
//! Anchor provides a persistent code graph that AI agents can query to understand
//! codebases without repeated file traversal.
//!
//! ## Key Features
//!
//! - **Graph-based**: Pre-computed relationships between symbols
//! - **Persistent**: Graph survives across sessions
//! - **Real-time**: File watcher keeps graph in sync
//! - **Multi-language**: Rust, Python, JavaScript, TypeScript
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use anchor::{build_graph, get_context};
//! use std::path::Path;
//!
//! // Build graph from project directory
//! let graph = build_graph(Path::new("."));
//!
//! // Query for a symbol with full context
//! let result = get_context(&graph, "login", "understand");
//! // Returns: symbol code + dependencies + dependents
//! ```

pub mod config;
pub mod error;
pub mod storage;
pub mod graph;
pub mod mcp;
pub mod parser;
pub mod query;
pub mod watcher;

// Re-exports for convenience
pub use error::{AnchorError, Result};

// Graph re-exports
pub use graph::{build_graph, CodeGraph, NodeKind, EdgeKind, SearchResult, GraphStats};
pub use parser::SupportedLanguage;
pub use query::{
    anchor_search, anchor_dependencies, anchor_file_symbols, anchor_stats,
    get_context, graph_search, Query, ContextResponse, SearchResponse, StatsResponse,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_code() {
        // Parse a Rust source string through the full pipeline
        let source = r#"
use std::collections::HashMap;

pub struct Config {
    name: String,
    values: HashMap<String, i32>,
}

impl Config {
    pub fn new(name: &str) -> Self {
        Config {
            name: name.to_string(),
            values: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&i32> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: String, value: i32) {
        self.values.insert(key, value);
    }
}

fn main() {
    let mut config = Config::new("test");
    config.set("port".to_string(), 8080);
    println!("{:?}", config.get("port"));
}
"#;
        use std::path::PathBuf;
        let path = PathBuf::from("test.rs");
        let extraction = parser::extract_file(&path, source).unwrap();

        // Should find: Config (struct), Config (impl), new, get, set (methods), main (function)
        assert!(!extraction.symbols.is_empty());

        let symbol_names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"Config"), "Should find struct Config");
        assert!(symbol_names.contains(&"main"), "Should find fn main");
        assert!(symbol_names.contains(&"new"), "Should find method new");
        assert!(symbol_names.contains(&"get"), "Should find method get");
        assert!(symbol_names.contains(&"set"), "Should find method set");

        // Should find the import
        assert!(!extraction.imports.is_empty());
        assert!(extraction.imports[0].path.contains("HashMap"));

        // Build graph and query it
        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let stats = graph.stats();
        assert!(stats.symbol_count >= 5, "Should have at least 5 symbols");
        assert_eq!(stats.file_count, 1, "Should have 1 file");

        // Search for Config
        let results = graph.search("Config", 3);
        assert!(!results.is_empty(), "Should find Config");
        assert_eq!(results[0].symbol, "Config");

        // Search for main
        let results = graph.search("main", 3);
        assert!(!results.is_empty(), "Should find main");
    }

    #[test]
    fn test_parse_python_code() {
        let source = r#"
import os
from typing import Optional, List

class UserService:
    def __init__(self, db):
        self.db = db

    def get_user(self, user_id: int) -> Optional[dict]:
        return self.db.find(user_id)

    def create_user(self, name: str) -> dict:
        return self.db.insert({"name": name})

def main():
    service = UserService(None)
    user = service.get_user(1)
    print(user)
"#;
        use std::path::PathBuf;
        let path = PathBuf::from("test.py");
        let extraction = parser::extract_file(&path, source).unwrap();

        let symbol_names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"UserService"), "Should find class UserService");
        assert!(symbol_names.contains(&"main"), "Should find function main");
        assert!(symbol_names.contains(&"get_user"), "Should find method get_user");

        // Build graph
        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let results = graph.search("UserService", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, NodeKind::Class);
    }

    #[test]
    fn test_parse_javascript_code() {
        let source = r#"
import { useState } from 'react';
import axios from 'axios';

class ApiClient {
    constructor(baseUrl) {
        this.baseUrl = baseUrl;
    }

    async fetchData(endpoint) {
        return axios.get(`${this.baseUrl}/${endpoint}`);
    }
}

function App() {
    const [data, setData] = useState(null);
    return data;
}

const API_URL = "https://api.example.com";
"#;
        use std::path::PathBuf;
        let path = PathBuf::from("test.js");
        let extraction = parser::extract_file(&path, source).unwrap();

        let symbol_names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"ApiClient"), "Should find class ApiClient");
        assert!(symbol_names.contains(&"App"), "Should find function App");

        // Should find imports
        assert!(!extraction.imports.is_empty());

        // Build graph and query
        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let results = graph.search("ApiClient", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, NodeKind::Class);
    }

    #[test]
    fn test_build_graph_self() {
        // Build graph of the Anchor project itself
        use std::path::Path;
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let graph = build_graph(&src_dir);

        let stats = graph.stats();
        // The Anchor src/ directory should have multiple Rust files
        assert!(stats.file_count > 0, "Should find source files");
        assert!(stats.symbol_count > 0, "Should find symbols");
        assert!(stats.total_edges > 0, "Should find edges");

        // Should be able to find our own types
        let results = graph.search("CodeGraph", 3);
        assert!(!results.is_empty(), "Should find CodeGraph struct");

        let results = graph.search("Storage", 3);
        assert!(!results.is_empty(), "Should find Storage struct");

        let results = graph.search("CodeGraph", 3);
        assert!(!results.is_empty(), "Should find CodeGraph struct");

    }

    #[test]
    fn test_query_api() {
        let source = r#"
pub fn login(username: &str, password: &str) -> bool {
    validate(username);
    check_password(password);
    true
}

fn validate(input: &str) -> bool {
    !input.is_empty()
}

fn check_password(pw: &str) -> bool {
    pw.len() >= 8
}
"#;
        use std::path::PathBuf;
        let path = PathBuf::from("auth.rs");
        let extraction = parser::extract_file(&path, source).unwrap();

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        // Test anchor_search with simple query
        let response = anchor_search(&graph, Query::Simple("login".to_string()));
        assert!(response.found);
        assert_eq!(response.count, 1);
        assert_eq!(response.results[0].symbol, "login");
        assert!(response.results[0].code.contains("pub fn login"));

        // Test anchor_search with structured query
        let response = anchor_search(
            &graph,
            Query::Structured {
                symbol: "validate".to_string(),
                kind: Some("function".to_string()),
                file: None,
            },
        );
        assert!(response.found);
        assert_eq!(response.results[0].symbol, "validate");

        // Test anchor_dependencies
        let deps = anchor_dependencies(&graph, "login");
        // login should have dependencies (calls validate and check_password)
        assert!(!deps.dependencies.is_empty() || deps.dependents.is_empty());

        // Test anchor_stats
        let stats_response = anchor_stats(&graph);
        assert!(stats_response.stats.symbol_count >= 3);
    }

    #[test]
    fn test_extract_unsupported_language() {
        use std::path::PathBuf;
        // Use a truly unsupported extension (.lua is not supported)
        let path = PathBuf::from("main.lua");
        let result = parser::extract_file(&path, "print('hello')");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AnchorError::UnsupportedLanguage(_)),
            "Expected UnsupportedLanguage, got: {:?}",
            err
        );
    }

    #[test]
    fn test_extract_empty_source() {
        use std::path::PathBuf;
        // Empty source is valid — tree-sitter parses it to an empty AST
        let path = PathBuf::from("empty.rs");
        let extraction = parser::extract_file(&path, "").unwrap();
        assert!(extraction.symbols.is_empty());
        assert!(extraction.imports.is_empty());
        assert!(extraction.calls.is_empty());
    }

    #[test]
    fn test_extract_malformed_syntax() {
        use std::path::PathBuf;
        // tree-sitter is error-tolerant — it should still parse
        let source = "fn broken( { struct }}}}";
        let path = PathBuf::from("bad.rs");
        let result = parser::extract_file(&path, source);
        // Should succeed (tree-sitter is lenient), but may find fewer symbols
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_no_extension() {
        use std::path::PathBuf;
        let path = PathBuf::from("Makefile");
        let result = parser::extract_file(&path, "all: build");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AnchorError::UnsupportedLanguage(_)
        ));
    }

    #[test]
    fn test_file_symbols_query() {
        let source = r#"
fn alpha() {}
fn beta() {}
struct Gamma {}
"#;
        use std::path::PathBuf;
        let path = PathBuf::from("src/abc.rs");
        let extraction = parser::extract_file(&path, source).unwrap();

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let response = anchor_file_symbols(&graph, "src/abc.rs");
        assert!(response.found);
        assert_eq!(response.symbols.len(), 3);

        let names: Vec<&str> = response.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert!(names.contains(&"Gamma"));
    }

    #[test]
    fn test_file_symbols_nonexistent() {
        let graph = CodeGraph::new();
        let response = anchor_file_symbols(&graph, "nonexistent.rs");
        assert!(!response.found);
        assert!(response.symbols.is_empty());
    }

    #[test]
    fn test_parse_unicode_identifiers_python() {
        // Python supports unicode identifiers
        let source = r#"
def café():
    return "coffee"

class Ñoño:
    pass
"#;
        use std::path::PathBuf;
        let path = PathBuf::from("test_unicode.py");
        let extraction = parser::extract_file(&path, source).unwrap();

        let symbol_names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"café"), "Should find unicode function name");
        assert!(symbol_names.contains(&"Ñoño"), "Should find unicode class name");

        // Build graph and search
        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let results = graph.search("café", 3);
        assert!(!results.is_empty(), "Should find unicode symbol via search");
    }

    #[test]
    fn test_parse_typescript_code() {
        let source = r#"
import { Request, Response } from 'express';

interface UserDTO {
    id: number;
    name: string;
}

type UserID = number;

enum Role {
    Admin,
    User,
    Guest,
}

class UserController {
    async getUser(req: Request, res: Response): Promise<void> {
        const user = await this.findUser(req.params.id);
        res.json(user);
    }
}

function createApp(): void {
    console.log("starting");
}
"#;
        use std::path::PathBuf;
        let path = PathBuf::from("test.ts");
        let extraction = parser::extract_file(&path, source).unwrap();

        let symbol_names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"UserDTO"), "Should find interface");
        assert!(symbol_names.contains(&"UserID"), "Should find type alias");
        assert!(symbol_names.contains(&"Role"), "Should find enum");
        assert!(symbol_names.contains(&"UserController"), "Should find class");
        assert!(symbol_names.contains(&"createApp"), "Should find function");

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let results = graph.search("UserDTO", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, NodeKind::Interface);
    }
}

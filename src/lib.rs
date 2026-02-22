//
//  lib.rs
//  Anchor
//
//  Created by hak (tharun)
//

pub mod cli;
pub mod config;
pub mod daemon;
pub mod error;
pub mod graph;
pub mod graphql;
pub mod lock;
pub mod mcp;
pub mod parser;
pub mod query;
pub mod regex;
pub mod storage;
pub mod updater;
pub mod watcher;
pub mod write;

// Re-exports for convenience
pub use error::{AnchorError, Result};

// Graph re-exports
pub use graph::{build_graph, CodeGraph, EdgeKind, GraphStats, NodeKind, SearchResult};
pub use parser::SupportedLanguage;
pub use query::{
    anchor_dependencies, anchor_file_symbols, anchor_search, anchor_stats, get_context,
    get_context_for_change, graph_search, ContextResponse, Edit, Query, Reference, SearchResponse,
    Signature, StatsResponse, Symbol,
};

// Write operations
pub use write::{
    create_file, insert_after, insert_before, replace_all, replace_first, WriteError, WriteResult,
};

// GraphQL
pub use graphql::{build_schema, execute, AnchorSchema};

// Regex engine (Brzozowski derivatives - ReDoS-safe)
pub use regex::{parse as parse_regex, Matcher as RegexMatcher, Regex};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_code() {
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

        assert!(!extraction.symbols.is_empty());

        let symbol_names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"Config"));
        assert!(symbol_names.contains(&"main"));
        assert!(symbol_names.contains(&"new"));
        assert!(symbol_names.contains(&"get"));
        assert!(symbol_names.contains(&"set"));

        assert!(!extraction.imports.is_empty());
        assert!(extraction.imports[0].path.contains("HashMap"));

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let stats = graph.stats();
        assert!(stats.symbol_count >= 5);
        assert_eq!(stats.file_count, 1);

        let results = graph.search("Config", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol, "Config");

        let results = graph.search("main", 3);
        assert!(!results.is_empty());
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
        assert!(symbol_names.contains(&"UserService"));
        assert!(symbol_names.contains(&"main"));
        assert!(symbol_names.contains(&"get_user"));

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
        assert!(symbol_names.contains(&"ApiClient"));
        assert!(symbol_names.contains(&"App"));

        assert!(!extraction.imports.is_empty());

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let results = graph.search("ApiClient", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, NodeKind::Class);
    }

    #[test]
    fn test_build_graph_self() {
        use std::path::Path;
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let graph = build_graph(&[src_dir.as_path()]);

        let stats = graph.stats();
        assert!(stats.file_count > 0);
        assert!(stats.symbol_count > 0);
        assert!(stats.total_edges > 0);

        let results = graph.search("CodeGraph", 3);
        assert!(!results.is_empty());

        let results = graph.search("Storage", 3);
        assert!(!results.is_empty());

        // Check call_lines are populated on symbols with graph dependencies
        let results = graph.search("build_from_extractions", 3);
        assert!(!results.is_empty());
        assert!(
            !results[0].call_lines.is_empty(),
            "call_lines should be populated for functions with graph dependencies"
        );
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

        let response = anchor_search(&graph, Query::Simple("login".to_string()));
        assert!(response.found);
        assert_eq!(response.count, 1);
        assert_eq!(response.results[0].symbol, "login");
        assert!(response.results[0].code.contains("pub fn login"));

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

        let deps = anchor_dependencies(&graph, "login");
        assert!(!deps.dependencies.is_empty() || deps.dependents.is_empty());

        let stats_response = anchor_stats(&graph);
        assert!(stats_response.stats.symbol_count >= 3);
    }

    #[test]
    fn test_extract_unsupported_language() {
        use std::path::PathBuf;
        let path = PathBuf::from("main.lua");
        let result = parser::extract_file(&path, "print('hello')");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AnchorError::UnsupportedLanguage(_)
        ));
    }

    #[test]
    fn test_extract_empty_source() {
        use std::path::PathBuf;
        let path = PathBuf::from("empty.rs");
        let extraction = parser::extract_file(&path, "").unwrap();
        assert!(extraction.symbols.is_empty());
        assert!(extraction.imports.is_empty());
        assert!(extraction.calls.is_empty());
    }

    #[test]
    fn test_extract_malformed_syntax() {
        use std::path::PathBuf;
        let source = "fn broken( { struct }}}";
        let path = PathBuf::from("bad.rs");
        let result = parser::extract_file(&path, source);
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
    fn test_get_context_intents() {
        // Build a graph with test scenarios
        let source = r#"
pub fn process(input: &str) -> String {
    validate(input);
    transform(input)
}

fn validate(s: &str) -> bool {
    !s.is_empty()
}

fn transform(s: &str) -> String {
    s.to_uppercase()
}

#[test]
fn test_process() {
    assert_eq!(process("hi"), "HI");
}
"#;
        use std::path::PathBuf;
        let path = PathBuf::from("src/lib.rs");
        let extraction = parser::extract_file(&path, source).unwrap();

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        // Test "explore" intent - understand the symbol
        let response = get_context(&graph, "validate", "explore");
        assert!(response.found);
        assert!(!response.symbols.is_empty());
        assert_eq!(response.intent, "explore");

        // Test "change" intent - what breaks if I modify
        let response = get_context(&graph, "validate", "change");
        assert!(response.found);
        assert_eq!(response.intent, "change");
        // Should have edits for dependents

        // Test "create" intent - patterns to follow
        let response = get_context(&graph, "validate", "create");
        assert!(response.found);
        assert_eq!(response.intent, "create");
        // Should find similar functions like transform
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
        assert!(
            symbol_names.contains(&"café"),
            "Should find unicode function name"
        );
        assert!(
            symbol_names.contains(&"Ñoño"),
            "Should find unicode class name"
        );

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
        assert!(
            symbol_names.contains(&"UserController"),
            "Should find class"
        );
        assert!(symbol_names.contains(&"createApp"), "Should find function");

        let mut graph = CodeGraph::new();
        graph.build_from_extractions(vec![extraction]);

        let results = graph.search("UserDTO", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].kind, NodeKind::Interface);
    }
}

#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn benchmark_search() {
        let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let graph = build_graph(&[repo_path.as_path()]);

        let start = std::time::Instant::now();
        let _result = graph_search(&graph, "CodeGraph", 2);
        let elapsed = start.elapsed();

        println!("Search benchmark: {}ms", elapsed.as_millis());
        assert!(elapsed.as_millis() < 100);
    }
}

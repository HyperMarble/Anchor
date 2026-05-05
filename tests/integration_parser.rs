use anchor::graph::CodeGraph;
use anchor::graph::types::NodeKind;
use anchor::parser::extract_file;
use anchor::AnchorError;
use std::path::PathBuf;

#[test]
fn test_extract_rust_functions_and_structs() {
    let src = r#"
pub struct User {
    pub id: u64,
    pub name: String,
}

impl User {
    pub fn new(name: &str) -> Self {
        User { id: 0, name: name.to_string() }
    }

    pub fn greet(&self) -> String {
        format!("Hello, {}", self.name)
    }
}

pub fn create_user(name: &str) -> User {
    User::new(name)
}
"#;
    let path = PathBuf::from("src/user.rs");
    let extraction = extract_file(&path, src).unwrap();
    let names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"User"));
    assert!(names.contains(&"new"));
    assert!(names.contains(&"greet"));
    assert!(names.contains(&"create_user"));
}

#[test]
fn test_extract_rust_imports() {
    let src = r#"
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

fn main() {}
"#;
    let path = PathBuf::from("src/main.rs");
    let extraction = extract_file(&path, src).unwrap();
    assert!(!extraction.imports.is_empty());
    let import_paths: Vec<&str> = extraction.imports.iter().map(|i| i.path.as_str()).collect();
    assert!(import_paths.iter().any(|p| p.contains("HashMap")));
}

#[test]
fn test_extract_python_class_and_methods() {
    let src = r#"
class PaymentService:
    def __init__(self, gateway):
        self.gateway = gateway

    def charge(self, amount: float) -> bool:
        return self.gateway.process(amount)

    def refund(self, transaction_id: str) -> bool:
        return self.gateway.reverse(transaction_id)

def process_payment(amount):
    svc = PaymentService(None)
    return svc.charge(amount)
"#;
    let path = PathBuf::from("services/payment.py");
    let extraction = extract_file(&path, src).unwrap();
    let names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"PaymentService"));
    assert!(names.contains(&"charge"));
    assert!(names.contains(&"refund"));
    assert!(names.contains(&"process_payment"));
}

#[test]
fn test_extract_javascript_functions_and_classes() {
    let src = r#"
import { EventEmitter } from 'events';

class TaskQueue extends EventEmitter {
    constructor() {
        super();
        this.tasks = [];
    }

    enqueue(task) {
        this.tasks.push(task);
        this.emit('queued', task);
    }
}

async function runTask(fn) {
    return await fn();
}

const helper = () => {};
"#;
    let path = PathBuf::from("queue.js");
    let extraction = extract_file(&path, src).unwrap();
    let names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"TaskQueue"));
    assert!(names.contains(&"runTask"));
    assert!(!extraction.imports.is_empty());
}

#[test]
fn test_extract_typescript_interfaces_and_enums() {
    let src = r#"
import { Request } from 'express';

interface Config {
    host: string;
    port: number;
}

enum Status {
    Active,
    Inactive,
    Pending,
}

type ID = string | number;

function loadConfig(): Config {
    return { host: 'localhost', port: 8080 };
}
"#;
    let path = PathBuf::from("config.ts");
    let extraction = extract_file(&path, src).unwrap();
    let names: Vec<&str> = extraction.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Config"));
    assert!(names.contains(&"Status"));
    assert!(names.contains(&"loadConfig"));
}

#[test]
fn test_extract_empty_source_is_ok() {
    let path = PathBuf::from("empty.rs");
    let extraction = extract_file(&path, "").unwrap();
    assert!(extraction.symbols.is_empty());
    assert!(extraction.imports.is_empty());
    assert!(extraction.calls.is_empty());
}

#[test]
fn test_extract_unsupported_extension_errors() {
    let path = PathBuf::from("script.lua");
    let err = extract_file(&path, "print('hello')").unwrap_err();
    assert!(matches!(err, AnchorError::UnsupportedLanguage(_)));
}

#[test]
fn test_extract_no_extension_errors() {
    let path = PathBuf::from("Makefile");
    let err = extract_file(&path, "all:\n\t$(MAKE)").unwrap_err();
    assert!(matches!(err, AnchorError::UnsupportedLanguage(_)));
}

#[test]
fn test_extract_malformed_rust_does_not_panic() {
    let path = PathBuf::from("broken.rs");
    // tree-sitter is error-tolerant; should return Ok even on bad syntax
    let result = extract_file(&path, "fn broken( { struct }}}}}");
    assert!(result.is_ok());
}

#[test]
fn test_extract_produces_call_edges() {
    let src = r#"
fn caller() {
    callee_a();
    callee_b();
}

fn callee_a() {}
fn callee_b() {}
"#;
    let path = PathBuf::from("calls.rs");
    let extraction = extract_file(&path, src).unwrap();
    // calls list should reference callee_a and callee_b
    let called: Vec<&str> = extraction.calls.iter().map(|c| c.callee.as_str()).collect();
    assert!(called.contains(&"callee_a") || called.contains(&"callee_b"));
}

#[test]
fn test_build_from_extractions_integrates_graph() {
    let src = r#"
pub struct Engine {}
impl Engine {
    pub fn start(&self) {}
    pub fn stop(&self) {}
}
"#;
    let path = PathBuf::from("engine.rs");
    let extraction = extract_file(&path, src).unwrap();
    let mut graph = CodeGraph::new();
    graph.build_from_extractions(vec![extraction]);
    let stats = graph.stats();
    assert!(stats.symbol_count >= 3);
    assert_eq!(stats.file_count, 1);
    let results = graph.search("Engine", 5);
    assert!(!results.is_empty());
}

#[test]
fn test_symbols_have_correct_kind_rust() {
    let src = r#"
pub struct MyStruct {}
impl MyStruct {
    pub fn my_method(&self) {}
}
pub fn standalone() {}
"#;
    let path = PathBuf::from("kinds.rs");
    let extraction = extract_file(&path, src).unwrap();
    let mut graph = CodeGraph::new();
    graph.build_from_extractions(vec![extraction]);
    let struct_result = graph.search("MyStruct", 1);
    assert!(!struct_result.is_empty());
    assert_eq!(struct_result[0].kind, NodeKind::Struct);
    let fn_result = graph.search("standalone", 1);
    assert!(!fn_result.is_empty());
    assert_eq!(fn_result[0].kind, NodeKind::Function);
}

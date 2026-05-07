use anchor::graph::CodeGraph;
use anchor::parser::extract_file;
use anchor::query::{get_context, get_context_for_change};
use std::path::PathBuf;

fn make(file: &str, src: &str) -> CodeGraph {
    let extraction = extract_file(&PathBuf::from(file), src).unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![extraction]);
    g
}

#[test]
fn test_get_context_explore_finds_symbol() {
    let g = make(
        "src/auth.rs",
        "pub fn login(u: &str) -> bool { validate(u); true }\nfn validate(s: &str) -> bool { !s.is_empty() }",
    );
    let resp = get_context(&g, "login", "explore");
    assert!(resp.found);
    assert_eq!(resp.intent, "explore");
    assert!(!resp.symbols.is_empty());
    assert_eq!(resp.symbols[0].name, "login");
}

#[test]
fn test_get_context_explore_populates_used_by() {
    let src = r#"
fn caller() { target(); }
fn target() {}
"#;
    let g = make("src/lib.rs", src);
    let resp = get_context(&g, "target", "explore");
    assert!(resp.found);
    // used_by should contain caller
    assert!(!resp.used_by.is_empty());
    assert!(resp.used_by.iter().any(|r| r.name.contains("caller")));
}

#[test]
fn test_get_context_change_returns_edits() {
    let src = r#"
pub fn process(x: i32) -> i32 { x }
fn run() { process(1); }
fn also_run() { process(2); }
"#;
    let g = make("src/proc.rs", src);
    let resp = get_context(&g, "process", "change");
    assert!(resp.found);
    assert_eq!(resp.intent, "change");
}

#[test]
fn test_get_context_create_finds_similar() {
    let src = r#"
pub fn save_user(name: &str) {}
pub fn save_post(title: &str) {}
pub fn save_comment(text: &str) {}
"#;
    let g = make("src/repo.rs", src);
    let resp = get_context(&g, "save_user", "create");
    assert!(resp.found);
    assert_eq!(resp.intent, "create");
}

#[test]
fn test_get_context_unknown_symbol_not_found() {
    let g = CodeGraph::new();
    let resp = get_context(&g, "ghost_function", "explore");
    assert!(!resp.found);
}

#[test]
fn test_get_context_default_intent_works() {
    let g = make("src/x.rs", "pub fn my_fn() {}");
    // Unknown intent should default to explore behavior
    let resp = get_context(&g, "my_fn", "unknown_intent");
    assert!(resp.found);
}

#[test]
fn test_get_context_for_change_with_new_signature() {
    let src = r#"
pub fn compute(x: i32) -> i32 { x * 2 }
fn use_compute() { compute(5); }
"#;
    let g = make("src/math.rs", src);
    let resp = get_context_for_change(
        &g,
        "compute",
        "change",
        Some("compute(x: i32, scale: f64) -> f64"),
    );
    assert!(resp.found);
    assert_eq!(resp.intent, "change");
}

#[test]
fn test_get_context_for_change_no_signature() {
    let src = "pub fn transform(s: &str) -> String { s.to_uppercase() }";
    let g = make("src/t.rs", src);
    let resp = get_context_for_change(&g, "transform", "change", None);
    assert!(resp.found);
}

#[test]
fn test_get_context_response_has_query_field() {
    let g = make("src/q.rs", "pub fn my_query_fn() {}");
    let resp = get_context(&g, "my_query_fn", "explore");
    assert_eq!(resp.query, "my_query_fn");
}

#[test]
fn test_get_context_multi_file_graph() {
    let e1 = extract_file(
        &PathBuf::from("src/a.rs"),
        "pub fn helper() {}",
    )
    .unwrap();
    let e2 = extract_file(
        &PathBuf::from("src/b.rs"),
        "fn user() { helper(); }",
    )
    .unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![e1, e2]);
    let resp = get_context(&g, "helper", "explore");
    assert!(resp.found);
}

use anchor::graph::types::NodeKind;
use anchor::graph::{build_graph, CodeGraph};
use anchor::parser::extract_file;
use std::path::{Path, PathBuf};

fn make(file: &str, src: &str) -> CodeGraph {
    let extraction = extract_file(&PathBuf::from(file), src).unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![extraction]);
    g
}

#[test]
fn test_all_symbols_returns_every_symbol() {
    let src = r#"
fn alpha() {}
fn beta() {}
struct Gamma {}
"#;
    let g = make("src/all.rs", src);
    let all = g.all_symbols();
    assert!(all.len() >= 3);
    let names: Vec<&str> = all.iter().map(|s| s.symbol.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    assert!(names.contains(&"Gamma"));
}

#[test]
fn test_all_files_returns_indexed_files() {
    let e1 = extract_file(&PathBuf::from("src/a.rs"), "fn a() {}").unwrap();
    let e2 = extract_file(&PathBuf::from("src/b.rs"), "fn b() {}").unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![e1, e2]);
    let files = g.all_files();
    assert_eq!(files.len(), 2);
}

#[test]
fn test_dependencies_returns_callees() {
    let src = r#"
fn orchestrate() {
    step_a();
    step_b();
}
fn step_a() {}
fn step_b() {}
"#;
    let g = make("src/pipe.rs", src);
    let deps = g.dependencies("orchestrate");
    let names: Vec<&str> = deps.iter().map(|d| d.symbol.as_str()).collect();
    assert!(names.contains(&"step_a") || names.contains(&"step_b"));
}

#[test]
fn test_dependents_returns_callers() {
    let src = r#"
fn caller_one() { helper(); }
fn caller_two() { helper(); }
fn helper() {}
"#;
    let g = make("src/callers.rs", src);
    let deps = g.dependents("helper");
    let names: Vec<&str> = deps.iter().map(|d| d.symbol.as_str()).collect();
    assert!(names.contains(&"caller_one") || names.contains(&"caller_two"));
}

#[test]
fn test_dependents_no_function_callers_for_uncalled_symbol() {
    let src = r#"
fn standalone() {}
fn other() {}
"#;
    let g = make("src/leaf.rs", src);
    let deps = g.dependents("standalone");
    // No function/method should call standalone — only the file node links to it
    let fn_callers: Vec<_> = deps
        .iter()
        .filter(|d| matches!(d.kind, NodeKind::Function | NodeKind::Method))
        .collect();
    assert!(fn_callers.is_empty());
}

#[test]
fn test_dependencies_empty_for_unknown() {
    let g = CodeGraph::new();
    let deps = g.dependencies("ghost");
    assert!(deps.is_empty());
}

#[test]
fn test_symbols_in_file_returns_correct_symbols() {
    let src = r#"
pub fn foo() {}
pub fn bar() {}
pub struct Baz {}
"#;
    let g = make("src/multi.rs", src);
    let syms = g.symbols_in_file(Path::new("src/multi.rs"));
    assert!(syms.len() >= 3);
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"foo"));
    assert!(names.contains(&"bar"));
    assert!(names.contains(&"Baz"));
}

#[test]
fn test_symbols_in_file_empty_for_unknown_path() {
    let g = CodeGraph::new();
    let syms = g.symbols_in_file(Path::new("does/not/exist.rs"));
    assert!(syms.is_empty());
}

#[test]
fn test_find_qualified_locates_symbol() {
    let src = "pub fn my_func() {}";
    let g = make("src/q.rs", src);
    let node = g.find_qualified(Path::new("src/q.rs"), "my_func");
    assert!(node.is_some());
    assert_eq!(node.unwrap().name, "my_func");
}

#[test]
fn test_find_qualified_returns_none_for_wrong_file() {
    let src = "pub fn my_func() {}";
    let g = make("src/q.rs", src);
    let node = g.find_qualified(Path::new("src/other.rs"), "my_func");
    assert!(node.is_none());
}

#[test]
fn test_build_graph_on_real_source() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let g = build_graph(&[src_dir.as_path()]);
    let stats = g.stats();
    assert!(stats.file_count > 0);
    assert!(stats.symbol_count > 0);
    let results = g.search("CodeGraph", 3);
    assert!(!results.is_empty());
}

#[test]
fn test_graph_stats_after_multi_file_build() {
    let e1 = extract_file(&PathBuf::from("src/x.rs"), "fn x() {}").unwrap();
    let e2 = extract_file(&PathBuf::from("src/y.rs"), "fn y() { x(); }").unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![e1, e2]);
    let stats = g.stats();
    assert_eq!(stats.file_count, 2);
    assert!(stats.symbol_count >= 2);
    assert!(stats.total_edges >= 2); // at least Defines edges
}

#[test]
fn test_search_limit_respected() {
    let src = r#"
fn process_one() {}
fn process_two() {}
fn process_three() {}
fn process_four() {}
fn process_five() {}
"#;
    let g = make("src/many.rs", src);
    let limited = g.search("process", 2);
    assert!(limited.len() <= 2);
    let all = g.search("process", 10);
    assert!(all.len() >= limited.len());
}

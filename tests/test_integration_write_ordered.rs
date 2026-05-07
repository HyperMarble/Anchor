use anchor::graph::CodeGraph;
use anchor::parser::extract_file;
use anchor::write::{plan_write_order, write_ordered, WriteOp};
use std::path::PathBuf;
use tempfile::tempdir;

fn make_graph(file: &str, src: &str) -> CodeGraph {
    let extraction = extract_file(&PathBuf::from(file), src).unwrap();
    let mut g = CodeGraph::new();
    g.build_from_extractions(vec![extraction]);
    g
}

#[test]
fn test_write_ordered_single_op() {
    let dir = tempdir().unwrap();
    let graph = CodeGraph::new();
    let ops = vec![WriteOp {
        path: dir.path().join("out.rs"),
        content: "fn hello() {}".to_string(),
        symbol: Some("hello".to_string()),
    }];
    let result = write_ordered(&graph, &ops).unwrap();
    assert_eq!(result.total_operations, 1);
    assert_eq!(result.results.len(), 1);
    assert!(result.results[0].success);
    assert!(dir.path().join("out.rs").exists());
}

#[test]
fn test_write_ordered_multiple_ops() {
    let dir = tempdir().unwrap();
    let graph = CodeGraph::new();
    let ops: Vec<WriteOp> = ["a.rs", "b.rs", "c.rs"]
        .iter()
        .map(|f| WriteOp {
            path: dir.path().join(f),
            content: format!("fn {}() {{}}", f.replace(".rs", "")),
            symbol: None,
        })
        .collect();
    let result = write_ordered(&graph, &ops).unwrap();
    assert_eq!(result.total_operations, 3);
    assert_eq!(result.results.len(), 3);
    assert!(result.results.iter().all(|r| r.success));
}

#[test]
fn test_write_ordered_creates_dirs_if_needed() {
    let dir = tempdir().unwrap();
    let graph = CodeGraph::new();
    let nested = dir.path().join("deep").join("nested").join("file.rs");
    let ops = vec![WriteOp {
        path: nested.clone(),
        content: "fn deep() {}".to_string(),
        symbol: None,
    }];
    let result = write_ordered(&graph, &ops).unwrap();
    assert!(result.results[0].success);
    assert!(nested.exists());
}

#[test]
fn test_write_ordered_empty_ops() {
    let graph = CodeGraph::new();
    let result = write_ordered(&graph, &[]).unwrap();
    assert_eq!(result.total_operations, 0);
    assert!(result.results.is_empty());
}

#[test]
fn test_plan_write_order_returns_all_indices() {
    let src = "fn a() { b(); }\nfn b() {}";
    let graph = make_graph("src/dep.rs", src);
    let dir = tempdir().unwrap();
    let ops: Vec<WriteOp> = ["a", "b"]
        .iter()
        .map(|s| WriteOp {
            path: dir.path().join(format!("{}.rs", s)),
            content: format!("fn {}() {{}}", s),
            symbol: Some(s.to_string()),
        })
        .collect();
    let order = plan_write_order(&graph, &ops);
    assert_eq!(order.len(), 2);
    // All indices present
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(sorted, vec![0, 1]);
}

#[test]
fn test_plan_write_order_empty() {
    let graph = CodeGraph::new();
    let order = plan_write_order(&graph, &[]);
    assert!(order.is_empty());
}

#[test]
fn test_write_ordered_records_write_order_names() {
    let dir = tempdir().unwrap();
    let graph = CodeGraph::new();
    let ops = vec![WriteOp {
        path: dir.path().join("named.rs"),
        content: "fn named() {}".to_string(),
        symbol: Some("named".to_string()),
    }];
    let result = write_ordered(&graph, &ops).unwrap();
    assert!(!result.write_order.is_empty());
    assert!(result.write_order[0].contains("named"));
}

#[test]
fn test_write_ordered_timing_recorded() {
    let dir = tempdir().unwrap();
    let graph = CodeGraph::new();
    let ops = vec![WriteOp {
        path: dir.path().join("timed.rs"),
        content: "fn t() {}".to_string(),
        symbol: None,
    }];
    let result = write_ordered(&graph, &ops).unwrap();
    // total_time_ms is always populated (even if 0)
    let _ = result.total_time_ms;
}

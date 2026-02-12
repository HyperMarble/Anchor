//! Plan execution: multi-operation plans from JSON
//!
//! Plans execute operations in parallel with automatic locking coordination.

use anyhow::Result;
use rayon::prelude::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::daemon::{send_request, Request, Response};
use crate::graph::CodeGraph;
use crate::write::{create_file, insert_after, replace_all, WriteError};
use super::read as cli_read;

/// Execute a plan file sequentially (fallback when no daemon)
pub fn execute(root: &Path, file: &str) -> Result<()> {
    let plan_path = if Path::new(file).is_absolute() {
        PathBuf::from(file)
    } else {
        root.join(file)
    };

    let content = std::fs::read_to_string(&plan_path)
        .map_err(|e| anyhow::anyhow!("Failed to read plan file: {}", e))?;

    let plan: PlanFile = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Invalid plan JSON: {}", e))?;

    // Load graph if any read operations exist
    let has_reads = plan.operations.iter().any(|op| matches!(op,
        PlanOperation::Search { .. } | PlanOperation::Read { .. } | PlanOperation::Context { .. }
    ));

    let graph = if has_reads {
        let cache_path = root.join(".anchor/graph.bin");
        CodeGraph::load(&cache_path).ok()
    } else {
        None
    };

    println!("Executing plan: {} operations", plan.operations.len());
    println!();

    let mut success_count = 0;
    let mut fail_count = 0;

    for (i, op) in plan.operations.iter().enumerate() {
        print!("[{}/{}] ", i + 1, plan.operations.len());

        let result = execute_operation(root, op, graph.as_ref());

        match result {
            Ok(_) => {
                println!("ok");
                success_count += 1;
            }
            Err(e) => {
                println!("FAILED: {}", e);
                fail_count += 1;
                if plan.stop_on_error.unwrap_or(false) {
                    println!("Stopping due to error (stop_on_error: true)");
                    break;
                }
            }
        }
    }

    println!();
    println!(
        "Plan complete: {} succeeded, {} failed",
        success_count, fail_count
    );

    Ok(())
}

fn execute_operation(root: &Path, op: &PlanOperation, graph: Option<&CodeGraph>) -> Result<(), WriteError> {
    match op {
        // ─── Read Operations ───────────────────────────────────────
        PlanOperation::Search { query, pattern, limit } => {
            print!("search {} ... ", query);
            if let Some(g) = graph {
                let _ = cli_read::search(g, &[query.clone()], pattern.as_deref(), limit.unwrap_or(20));
            }
            Ok(())
        }
        PlanOperation::Read { symbol } => {
            print!("read {} ... ", symbol);
            if let Some(g) = graph {
                let _ = cli_read::read(g, symbol);
            }
            Ok(())
        }
        PlanOperation::Context { query, limit } => {
            print!("context {} ... ", query);
            if let Some(g) = graph {
                let _ = cli_read::context(g, &[query.clone()], limit.unwrap_or(5));
            }
            Ok(())
        }
        // ─── Write Operations ──────────────────────────────────────
        PlanOperation::Create { path, content } => {
            print!("create {} ... ", path);
            let p = root.join(path);
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            create_file(&p, content).map(|_| ())
        }
        PlanOperation::Insert {
            path,
            pattern,
            content,
        } => {
            print!("insert into {} ... ", path);
            insert_after(&root.join(path), pattern, content).map(|_| ())
        }
        PlanOperation::Replace { path, old, new } => {
            print!("replace in {} ... ", path);
            replace_all(&root.join(path), old, new).map(|_| ())
        }
        PlanOperation::Delete { path } => {
            print!("delete {} ... ", path);
            std::fs::remove_file(root.join(path)).map_err(WriteError::IoError)
        }
    }
}

// ─── Plan File Types ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PlanFile {
    pub operations: Vec<PlanOperation>,
    #[serde(default)]
    pub stop_on_error: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
pub enum PlanOperation {
    // ─── Read Operations (no locking needed) ───────────────────
    #[serde(rename = "search")]
    Search { query: String, pattern: Option<String>, limit: Option<usize> },

    #[serde(rename = "read")]
    Read { symbol: String },

    #[serde(rename = "context")]
    Context { query: String, limit: Option<usize> },

    // ─── Write Operations (with locking) ───────────────────────
    #[serde(rename = "create")]
    Create { path: String, content: String },

    #[serde(rename = "insert")]
    Insert {
        path: String,
        pattern: String,
        content: String,
    },

    #[serde(rename = "replace")]
    Replace { path: String, old: String, new: String },

    #[serde(rename = "delete")]
    Delete { path: String },
}

// ─── Parallel Execution (via daemon with locking) ──────────────

/// Execute a plan file with parallel operations via daemon (with locking)
pub fn execute_parallel(root: &Path, file: &str) -> Result<()> {
    let plan_path = if Path::new(file).is_absolute() {
        PathBuf::from(file)
    } else {
        root.join(file)
    };

    let content = std::fs::read_to_string(&plan_path)
        .map_err(|e| anyhow::anyhow!("Failed to read plan file: {}", e))?;

    let plan: PlanFile = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Invalid plan JSON: {}", e))?;

    // Load graph if any read operations exist
    let has_reads = plan.operations.iter().any(|op| matches!(op,
        PlanOperation::Search { .. } | PlanOperation::Read { .. } | PlanOperation::Context { .. }
    ));

    let graph = if has_reads {
        let cache_path = root.join(".anchor/graph.bin");
        CodeGraph::load(&cache_path).ok()
    } else {
        None
    };

    println!(
        "Executing plan: {} operations (parallel with locking)",
        plan.operations.len()
    );
    println!();

    let success_count = AtomicUsize::new(0);
    let fail_count = AtomicUsize::new(0);

    // Execute operations in parallel - locking handles coordination
    let results: Vec<(usize, &PlanOperation, Result<Response, String>)> = plan
        .operations
        .par_iter()
        .enumerate()
        .map(|(i, op)| {
            let result = execute_operation_via_daemon(root, op, graph.as_ref());
            (i, op, result)
        })
        .collect();

    // Print results in order
    for (i, op, result) in results {
        let op_desc = match op {
            PlanOperation::Search { query, .. } => format!("search {}", query),
            PlanOperation::Read { symbol } => format!("read {}", symbol),
            PlanOperation::Context { query, .. } => format!("context {}", query),
            PlanOperation::Create { path, .. } => format!("create {}", path),
            PlanOperation::Insert { path, .. } => format!("insert {}", path),
            PlanOperation::Replace { path, .. } => format!("replace {}", path),
            PlanOperation::Delete { path } => format!("delete {}", path),
        };

        match result {
            Ok(Response::Ok { .. }) => {
                println!("[{}/{}] {} ... ok", i + 1, plan.operations.len(), op_desc);
                success_count.fetch_add(1, Ordering::Relaxed);
            }
            Ok(Response::Error { message }) => {
                println!(
                    "[{}/{}] {} ... FAILED: {}",
                    i + 1,
                    plan.operations.len(),
                    op_desc,
                    message
                );
                fail_count.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                println!(
                    "[{}/{}] {} ... FAILED: {}",
                    i + 1,
                    plan.operations.len(),
                    op_desc,
                    e
                );
                fail_count.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                println!(
                    "[{}/{}] {} ... unexpected response",
                    i + 1,
                    plan.operations.len(),
                    op_desc
                );
                fail_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    println!();
    println!(
        "Plan complete: {} succeeded, {} failed",
        success_count.load(Ordering::Relaxed),
        fail_count.load(Ordering::Relaxed)
    );

    Ok(())
}

fn execute_operation_via_daemon(root: &Path, op: &PlanOperation, graph: Option<&CodeGraph>) -> Result<Response, String> {
    // Read operations don't need daemon - execute directly
    match op {
        PlanOperation::Search { query, pattern, limit } => {
            if let Some(g) = graph {
                let _ = cli_read::search(g, &[query.clone()], pattern.as_deref(), limit.unwrap_or(20));
            }
            return Ok(Response::Ok { data: serde_json::json!({"op": "search"}) });
        }
        PlanOperation::Read { symbol } => {
            if let Some(g) = graph {
                let _ = cli_read::read(g, symbol);
            }
            return Ok(Response::Ok { data: serde_json::json!({"op": "read"}) });
        }
        PlanOperation::Context { query, limit } => {
            if let Some(g) = graph {
                let _ = cli_read::context(g, &[query.clone()], limit.unwrap_or(5));
            }
            return Ok(Response::Ok { data: serde_json::json!({"op": "context"}) });
        }
        _ => {}
    }

    // Write operations go through daemon
    let request = match op {
        PlanOperation::Create { path, content } => Request::Create {
            path: path.clone(),
            content: content.clone(),
        },
        PlanOperation::Insert {
            path,
            pattern,
            content,
        } => Request::Insert {
            path: path.clone(),
            pattern: pattern.clone(),
            content: content.clone(),
        },
        PlanOperation::Replace { path, old, new } => Request::Replace {
            path: path.clone(),
            old: old.clone(),
            new: new.clone(),
        },
        PlanOperation::Delete { path } => {
            return match std::fs::remove_file(root.join(path)) {
                Ok(_) => Ok(Response::Ok {
                    data: serde_json::json!({"deleted": path}),
                }),
                Err(e) => Ok(Response::Error {
                    message: e.to_string(),
                }),
            };
        }
        _ => return Ok(Response::Ok { data: serde_json::json!({}) }),
    };

    send_request(root, request).map_err(|e| e.to_string())
}

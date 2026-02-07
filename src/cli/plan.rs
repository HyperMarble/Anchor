//! Plan execution: multi-operation plans from JSON
//!
//! Plans execute operations in parallel with automatic locking coordination.

use anyhow::Result;
use rayon::prelude::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::daemon::{send_request, Request, Response};
use crate::write::{create_file, insert_after, replace_all, WriteError};

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

    println!("Executing plan: {} operations", plan.operations.len());
    println!();

    let mut success_count = 0;
    let mut fail_count = 0;

    for (i, op) in plan.operations.iter().enumerate() {
        print!("[{}/{}] ", i + 1, plan.operations.len());

        let result = execute_operation(root, op);

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

fn execute_operation(root: &Path, op: &PlanOperation) -> Result<(), WriteError> {
    match op {
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
            let result = execute_operation_via_daemon(root, op);
            (i, op, result)
        })
        .collect();

    // Print results in order
    for (i, op, result) in results {
        let op_desc = match op {
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

fn execute_operation_via_daemon(root: &Path, op: &PlanOperation) -> Result<Response, String> {
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
            // Delete isn't in daemon protocol yet - do it directly
            return match std::fs::remove_file(root.join(path)) {
                Ok(_) => Ok(Response::Ok {
                    data: serde_json::json!({"deleted": path}),
                }),
                Err(e) => Ok(Response::Error {
                    message: e.to_string(),
                }),
            };
        }
    };

    send_request(root, request).map_err(|e| e.to_string())
}

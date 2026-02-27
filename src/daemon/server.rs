//
//  server.rs
//  Anchor
//
//  Created by hak (tharun)
//

use anyhow::Result;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use tracing::{debug, error, info, warn};

use crate::graph::engine::CodeGraph;
use crate::graph::rebuild_file;
use crate::lock::{LockManager, LockStatus};
use crate::watcher::{start_watching, WatcherHandle};
use crate::write;
use crate::{anchor_dependencies, anchor_stats, build_graph, get_context, graph_search};

use super::protocol::{Request, Response};

/// Default socket path (in project's .anchor directory)
pub fn socket_path(root: &Path) -> PathBuf {
    root.join(".anchor").join("anchor.sock")
}

/// PID file path
pub fn pid_path(root: &Path) -> PathBuf {
    root.join(".anchor").join("daemon.pid")
}

/// Start the daemon server.
pub fn start_daemon(roots: &[PathBuf]) -> Result<()> {
    let roots: Vec<PathBuf> = roots
        .iter()
        .map(|r| r.canonicalize())
        .collect::<Result<Vec<_>, _>>()?;
    let primary_root = roots[0].clone();
    let sock_path = socket_path(&primary_root);
    let pid_file = pid_path(&primary_root);

    // Ensure .anchor directory exists
    std::fs::create_dir_all(sock_path.parent().unwrap())?;

    // Remove stale socket if exists
    if sock_path.exists() {
        std::fs::remove_file(&sock_path)?;
    }

    // Write PID file
    std::fs::write(&pid_file, std::process::id().to_string())?;

    // Build initial graph
    info!(roots = ?roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>(), "building initial graph");
    let root_refs: Vec<&Path> = roots.iter().map(|r| r.as_path()).collect();
    let graph = build_graph(&root_refs);
    let graph = Arc::new(RwLock::new(graph));

    // Create lock manager
    let lock_manager = Arc::new(LockManager::new());
    info!("lock manager initialized");

    // Start file watcher for each root
    let _watchers: Vec<Option<WatcherHandle>> = roots
        .iter()
        .map(|root| match start_watching(root, Arc::clone(&graph), 200) {
            Ok(handle) => {
                info!(root = %root.display(), "file watcher started");
                Some(handle)
            }
            Err(e) => {
                warn!(root = %root.display(), error = %e, "file watcher failed to start");
                None
            }
        })
        .collect();

    // Bind socket
    let listener = UnixListener::bind(&sock_path)?;
    info!(socket = %sock_path.display(), "daemon listening");

    // Shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));

    // Accept connections
    for stream in listener.incoming() {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        match stream {
            Ok(stream) => {
                let graph = Arc::clone(&graph);
                let shutdown = Arc::clone(&shutdown);
                let lock_manager = Arc::clone(&lock_manager);
                let root = primary_root.clone();
                let root_refs: Vec<PathBuf> = roots.clone();

                thread::spawn(move || {
                    if let Err(e) =
                        handle_client(stream, &graph, &lock_manager, &shutdown, &root, &root_refs)
                    {
                        debug!(error = %e, "client handler error");
                    }
                });
            }
            Err(e) => {
                error!(error = %e, "accept error");
            }
        }
    }

    // Cleanup
    info!("daemon shutting down");
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_file(&pid_file);

    Ok(())
}

/// Handle a single client connection.
fn handle_client(
    stream: UnixStream,
    graph: &Arc<RwLock<CodeGraph>>,
    lock_manager: &Arc<LockManager>,
    shutdown: &Arc<AtomicBool>,
    root: &Path,
    roots: &[PathBuf],
) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    let mut line = String::new();
    reader.read_line(&mut line)?;

    let request: Request = serde_json::from_str(&line)?;
    debug!(?request, "received request");

    let response = process_request(request, graph, lock_manager, shutdown, root, roots);

    let response_json = serde_json::to_string(&response)?;
    writeln!(writer, "{}", response_json)?;

    Ok(())
}

/// Process a request and return a response.
fn process_request(
    request: Request,
    graph: &Arc<RwLock<CodeGraph>>,
    lock_manager: &Arc<LockManager>,
    shutdown: &Arc<AtomicBool>,
    root: &Path,
    roots: &[PathBuf],
) -> Response {
    match request {
        Request::Ping => Response::Pong,

        Request::Shutdown => {
            shutdown.store(true, Ordering::Relaxed);
            // Wake the blocking accept loop so it can observe shutdown and exit.
            let _ = UnixStream::connect(socket_path(root));
            Response::Goodbye
        }

        // ─── Read Operations ───────────────────────────────────
        Request::Stats => {
            let g = match graph.read() {
                Ok(g) => g,
                Err(e) => return Response::error(format!("lock error: {}", e)),
            };
            let result = anchor_stats(&g);
            Response::ok(result)
        }

        Request::Search { query, depth } => {
            let g = match graph.read() {
                Ok(g) => g,
                Err(e) => return Response::error(format!("lock error: {}", e)),
            };
            let result = graph_search(&g, &query, depth);
            Response::ok(result)
        }

        Request::Context { query, intent } => {
            let g = match graph.read() {
                Ok(g) => g,
                Err(e) => return Response::error(format!("lock error: {}", e)),
            };
            let result = get_context(&g, &query, &intent);
            Response::ok(result)
        }

        Request::Deps { symbol } => {
            let g = match graph.read() {
                Ok(g) => g,
                Err(e) => return Response::error(format!("lock error: {}", e)),
            };
            let result = anchor_dependencies(&g, &symbol);
            Response::ok(result)
        }

        Request::Overview => {
            let g = match graph.read() {
                Ok(g) => g,
                Err(e) => return Response::error(format!("lock error: {}", e)),
            };
            let stats = g.stats();
            let files = graph_search(&g, "src/", 0);
            let mains = graph_search(&g, "main", 0);
            Response::ok(serde_json::json!({
                "stats": stats,
                "files": files.matched_files,
                "entry_points": mains.symbols.iter()
                    .filter(|s| s.name == "main")
                    .collect::<Vec<_>>()
            }))
        }

        // ─── Write Operations (with locking) ───────────────────
        Request::Create { path, content } => {
            let file_path = root.join(&path);
            with_file_lock(&file_path, graph, lock_manager, |fp| {
                if let Some(parent) = fp.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let wr = write::create_file(fp, &content)?;
                Ok(serde_json::json!({
                    "success": true, "path": wr.path, "lines_written": wr.lines_written
                }))
            })
        }

        Request::Insert {
            path,
            pattern,
            content,
        } => {
            let file_path = root.join(&path);
            with_file_lock(&file_path, graph, lock_manager, |fp| {
                let wr = write::insert_after(fp, &pattern, &content)?;
                Ok(serde_json::json!({
                    "success": true, "path": wr.path, "lines_written": wr.lines_written
                }))
            })
        }

        Request::Replace { path, old, new } => {
            let file_path = root.join(&path);
            with_file_lock(&file_path, graph, lock_manager, |fp| {
                let wr = write::replace_all(fp, &old, &new)?;
                Ok(serde_json::json!({
                    "success": true, "path": wr.path, "replacements": wr.replacements
                }))
            })
        }

        // ─── Lock Management ───────────────────────────────────
        Request::LockStatus { path } => {
            let file_path = root.join(&path);
            match lock_manager.status(&file_path) {
                LockStatus::Unlocked => Response::ok(serde_json::json!({
                    "locked": false,
                    "path": path
                })),
                LockStatus::Locked { by, duration_ms } => Response::ok(serde_json::json!({
                    "locked": true,
                    "path": path,
                    "locked_by": by.to_string(),
                    "locked_by_symbol": by.name,
                    "duration_ms": duration_ms
                })),
            }
        }

        Request::Locks => {
            let locks = lock_manager.active_locks();
            let lock_infos: Vec<_> = locks
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "primary_symbol": l.primary_symbol.to_string(),
                        "locked_symbols": l.locked_symbols.iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>(),
                        "duration_ms": l.duration_ms
                    })
                })
                .collect();
            Response::ok(serde_json::json!({
                "count": locks.len(),
                "locks": lock_infos
            }))
        }

        // ─── Symbol Locking ────────────────────────────────────
        Request::LockSymbol { file, symbol } => {
            let file_path = root.join(&file);
            let g = match graph.read() {
                Ok(g) => g,
                Err(e) => return Response::error(format!("graph lock error: {}", e)),
            };
            let key = crate::lock::SymbolKey::new(file_path, symbol);
            match lock_manager.acquire_symbol_with_wait(
                &key,
                &g,
                std::time::Duration::from_secs(30),
            ) {
                crate::lock::LockResult::Acquired {
                    symbol, dependents, ..
                }
                | crate::lock::LockResult::AcquiredAfterWait {
                    symbol, dependents, ..
                } => Response::ok(serde_json::json!({
                    "locked": true,
                    "symbol": symbol.to_string(),
                    "dependents": dependents.iter().map(|d| d.to_string()).collect::<Vec<_>>()
                })),
                crate::lock::LockResult::Blocked { blocked_by, reason } => {
                    Response::error(format!("Blocked by {}: {}", blocked_by, reason))
                }
            }
        }

        Request::UnlockSymbol { file, symbol } => {
            let file_path = root.join(&file);
            let key = crate::lock::SymbolKey::new(file_path, symbol);
            lock_manager.release_symbol(&key);
            Response::ok(serde_json::json!({ "unlocked": true }))
        }

        // ─── System ────────────────────────────────────────────
        Request::Rebuild => {
            let root_refs: Vec<&Path> = roots.iter().map(|r| r.as_path()).collect();
            let new_graph = build_graph(&root_refs);
            let mut g = match graph.write() {
                Ok(g) => g,
                Err(e) => return Response::error(format!("lock error: {}", e)),
            };
            *g = new_graph;
            let stats = g.stats();
            Response::ok(serde_json::json!({
                "message": "graph rebuilt",
                "stats": stats
            }))
        }
    }
}

/// Check if daemon is running by checking PID file and process.
pub fn is_daemon_running(root: &Path) -> bool {
    let pid_file = pid_path(root);

    if !pid_file.exists() {
        return false;
    }

    // Read PID and check if process is alive
    if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            // Check if process exists (signal 0 = check existence)
            unsafe {
                return libc::kill(pid, 0) == 0;
            }
        }
    }

    false
}

/// Acquire a file lock, run a write operation, release the lock.
fn with_file_lock<F>(
    file_path: &Path,
    graph: &Arc<RwLock<CodeGraph>>,
    lock_manager: &Arc<LockManager>,
    write_fn: F,
) -> Response
where
    F: FnOnce(&Path) -> Result<serde_json::Value, write::WriteError>,
{
    let g = match graph.read() {
        Ok(g) => g,
        Err(e) => return Response::error(format!("graph lock error: {}", e)),
    };

    let lock_result =
        lock_manager.acquire_with_wait(file_path, &g, std::time::Duration::from_secs(30));
    drop(g);

    match lock_result {
        crate::lock::LockResult::Acquired { dependents, .. }
        | crate::lock::LockResult::AcquiredAfterWait { dependents, .. } => {
            let result = write_fn(file_path);
            lock_manager.release(file_path);

            match result {
                Ok(mut data) => {
                    // Keep daemon graph fresh immediately after successful writes.
                    if let Ok(mut g) = graph.write() {
                        let _ = rebuild_file(&mut g, file_path);
                    }
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert("locked_dependents".to_string(), dependents.len().into());
                    }
                    Response::ok(data)
                }
                Err(e) => Response::error(format!("write error: {}", e)),
            }
        }
        crate::lock::LockResult::Blocked { blocked_by, reason } => {
            Response::error(format!("Blocked by {}: {}", blocked_by, reason))
        }
    }
}

/// Send a request to the daemon and get a response.
pub fn send_request(root: &Path, request: Request) -> Result<Response> {
    let sock_path = socket_path(root);
    let mut stream = UnixStream::connect(&sock_path)?;

    let request_json = serde_json::to_string(&request)?;
    writeln!(stream, "{}", request_json)?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: Response = serde_json::from_str(&response_line)?;
    Ok(response)
}

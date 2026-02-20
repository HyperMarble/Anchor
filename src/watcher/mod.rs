//
//  mod.rs
//  Anchor
//
//  Created by hak (tharun)
//

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::graph::builder::rebuild_file;
use crate::graph::engine::CodeGraph;
use crate::parser::SupportedLanguage;

/// Default debounce duration for file events.
const DEFAULT_DEBOUNCE_MS: u64 = 200;

/// Directories to always ignore.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".anchor",
    "__pycache__",
    ".venv",
    "dist",
    "build",
];

/// Start watching a directory for file changes, updating the graph in real-time.
///
/// Returns a handle that keeps the watcher alive. Drop it to stop watching.
///
/// # Arguments
/// * `root` - The directory to watch recursively
/// * `graph` - Shared graph to update on changes
/// * `debounce_ms` - Debounce duration in milliseconds (0 = use default 200ms)
pub fn start_watching(
    root: &Path,
    graph: Arc<RwLock<CodeGraph>>,
    debounce_ms: u64,
) -> Result<WatcherHandle, notify::Error> {
    let debounce = if debounce_ms == 0 {
        Duration::from_millis(DEFAULT_DEBOUNCE_MS)
    } else {
        Duration::from_millis(debounce_ms)
    };

    let root_owned = root.to_path_buf();

    let mut debouncer = new_debouncer(
        debounce,
        move |result: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
            match result {
                Ok(events) => {
                    handle_events(&events, &graph, &root_owned);
                }
                Err(e) => {
                    warn!(error = %e, "file watcher error");
                }
            }
        },
    )?;

    debouncer
        .watcher()
        .watch(root, notify::RecursiveMode::Recursive)?;

    info!(root = %root.display(), debounce_ms = debounce.as_millis() as u64, "file watcher started");

    Ok(WatcherHandle {
        _debouncer: debouncer,
    })
}

/// Handle debounced file events.
fn handle_events(
    events: &[notify_debouncer_mini::DebouncedEvent],
    graph: &Arc<RwLock<CodeGraph>>,
    _root: &Path,
) {
    // Deduplicate: collect unique paths and their last event kind
    let mut paths: std::collections::HashMap<PathBuf, DebouncedEventKind> =
        std::collections::HashMap::new();

    for event in events {
        let path = &event.path;

        // Skip ignored directories
        if should_ignore(path) {
            continue;
        }

        // Only process source files
        if SupportedLanguage::from_path(path).is_none() {
            continue;
        }

        paths.insert(path.clone(), event.kind);
    }

    if paths.is_empty() {
        return;
    }

    debug!(count = paths.len(), "processing file events");

    let mut graph = match graph.write() {
        Ok(g) => g,
        Err(e) => {
            warn!(error = %e, "failed to acquire graph write lock");
            return;
        }
    };

    for (path, kind) in &paths {
        match kind {
            DebouncedEventKind::Any => {
                if path.exists() {
                    // File was created or modified — rebuild
                    debug!(file = %path.display(), "rebuilding changed file");
                    if let Err(e) = rebuild_file(&mut graph, path) {
                        warn!(file = %path.display(), error = %e, "rebuild failed");
                    }
                } else {
                    // File was deleted — remove
                    debug!(file = %path.display(), "removing deleted file");
                    graph.remove_file(path);
                }
            }
            DebouncedEventKind::AnyContinuous => {
                // Ongoing writes — skip until settled
                debug!(file = %path.display(), "skipping continuous write");
            }
            _ => {
                debug!(file = %path.display(), "unhandled event kind");
            }
        }
    }
}

/// Check if a path should be ignored (hidden dirs, build dirs, etc.).
fn should_ignore(path: &Path) -> bool {
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if IGNORED_DIRS.contains(&name.as_ref()) {
                return true;
            }
        }
    }
    false
}

/// Handle that keeps the file watcher alive.
/// Drop this to stop watching.
pub struct WatcherHandle {
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

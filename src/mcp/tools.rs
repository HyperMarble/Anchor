//
//  tools.rs
//  Anchor
//
//  Created by hak (tharun)
//

use rmcp::{handler::server::wrapper::Parameters, model::*, tool, tool_router};
use std::sync::Arc;

use super::format::short_kind;
use super::types::*;
use super::AnchorMcp;
use crate::cache::PersistentCache;
use crate::lock::{lockd, LockResult, SymbolKey};
use crate::query::slice::slice_code;
use crate::storage::{AnchorStore, ANCHOR_DIR};

#[tool_router]
impl AnchorMcp {
    pub fn new(roots: Vec<std::path::PathBuf>) -> Self {
        let root = roots[0].clone();
        let store = AnchorStore::discover(&root)
            .or_else(|_| AnchorStore::init(&root))
            .expect("failed to open/init anchor store");

        let anchor_root = root.join(ANCHOR_DIR);
        let persistent_cache = PersistentCache::load(&anchor_root);

        Self {
            root,
            tool_router: Self::tool_router(),
            store: Arc::new(store),
            lock_manager: Arc::new(crate::lock::LockManager::new()),
            session_seen: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            persistent_cache: Arc::new(std::sync::Mutex::new(persistent_cache)),
        }
    }

    fn err(msg: impl Into<String>) -> ErrorData {
        ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: std::borrow::Cow::from(msg.into()),
            data: None,
        }
    }

    #[tool(
        description = "Get context for symbols. Adaptive: use signature:true for just the declaration line, full:true for every line, callers/callees:false to skip call graph. Supports multiple symbols in one call. Set bundle:true to automatically include call-graph neighbors not yet seen this session."
    )]
    async fn context(
        &self,
        Parameters(req): Parameters<ContextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = &self.store;
        let limit = req.limit.unwrap_or(5);
        let bundle = req.bundle.unwrap_or(false);
        let signature_only = req.signature.unwrap_or(false);
        let show_callers = req.callers.unwrap_or(true);
        let show_callees = req.callees.unwrap_or(true);
        let call_index = store.load_call_index();
        let mut output = String::new();
        let mut bundled_names: Vec<String> = Vec::new();

        for (i, query) in req.symbols.iter().enumerate() {
            if i > 0 {
                output.push_str("\n===\n");
            }

            let candidates = store
                .search_symbols_hybrid(query, limit)
                .map_err(|e| Self::err(e.to_string()))?;

            if candidates.is_empty() {
                output.push_str(&format!("No results for '{}'\n", query));
                continue;
            }

            for sym in &candidates {
                // Cross-session cache: symbol unchanged since last session
                {
                    let mut cache = self.persistent_cache.lock().unwrap();
                    if cache.is_hit(&sym.name, &sym.path, &sym.slice_hash) {
                        output.push_str(&format!(
                            "CACHED {} {} {}:{}\n",
                            sym.name, sym.kind, sym.path, sym.line_start
                        ));
                        continue;
                    }
                    cache.update(&sym.name, &sym.path, &sym.slice_hash);
                }

                // Within-session dedup: already sent this symbol this session
                let dedup_key = (sym.name.clone(), sym.slice_hash.clone());
                {
                    let mut seen = self.session_seen.lock().unwrap();
                    if seen.contains(&dedup_key) {
                        output.push_str(&format!(
                            "CACHED {} {} {}:{}\n",
                            sym.name, sym.kind, sym.path, sym.line_start
                        ));
                        continue;
                    }
                    seen.insert(dedup_key);
                }

                let proj = match store.create_projection(sym) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                output.push_str(&format!(
                    "{} {} {}:{}\n",
                    sym.name, sym.kind, sym.path, sym.line_start
                ));

                if signature_only {
                    // Just the declaration line — no body, no call graph
                    let sig = proj.text.lines().next().unwrap_or("").trim_end();
                    output.push_str(&format!(" {:>4}: {}\n\n", sym.line_start, sig));
                    continue;
                }

                let callers = call_index.callers_of(&sym.name);
                let callees = call_index.callees_of(&sym.name);

                if show_callers && !callers.is_empty() {
                    output.push_str(&format!(
                        "  CALLED_BY: {}\n",
                        callers
                            .iter()
                            .take(8)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if show_callees && !callees.is_empty() {
                    output.push_str(&format!(
                        "  CALLS: {}\n",
                        callees
                            .iter()
                            .take(8)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }

                let sliced = slice_code(&proj.text, &[], sym.line_start);
                if sliced.was_sliced {
                    output.push_str(&format!(
                        "  [{}/{} lines]\n",
                        sliced.shown_lines, sliced.total_lines
                    ));
                }
                for (j, line) in sliced.code.lines().enumerate() {
                    output.push_str(&format!(" {:>4}: {}\n", sym.line_start + j, line));
                }
                output.push('\n');

                // collect callees for bundling
                if bundle {
                    for callee in callees.iter().take(8) {
                        bundled_names.push(callee.to_string());
                    }
                }
            }
        }

        // bundle: fetch unseen callee symbols and append
        if bundle && !bundled_names.is_empty() {
            let mut bundle_output = String::new();
            let mut already_bundled: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            for callee in &bundled_names {
                if !already_bundled.insert(callee.clone()) {
                    continue;
                }
                // skip stdlib/primitive names: only bundle callees that are themselves
                // callers in our index (i.e. project-defined functions with their own body)
                if call_index.callees_of(callee).is_empty() {
                    continue;
                }
                // skip universal names that match unrelated code across the whole repo
                const SKIP: &[&str] = &[
                    "new",
                    "from",
                    "into",
                    "clone",
                    "default",
                    "collect",
                    "iter",
                    "map",
                    "filter",
                    "unwrap",
                    "expect",
                    "to_string",
                    "as_str",
                    "as_ref",
                    "drop",
                ];
                if SKIP.contains(&callee.as_str()) {
                    continue;
                }
                let neighbors = match store.search_symbols_hybrid(callee, 2) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                for sym in &neighbors {
                    if sym.name != *callee {
                        continue; // exact match only for bundle
                    }
                    let dedup_key = (sym.name.clone(), sym.slice_hash.clone());
                    {
                        let mut seen = self.session_seen.lock().unwrap();
                        if seen.contains(&dedup_key) {
                            continue; // already returned this session
                        }
                        seen.insert(dedup_key);
                    }
                    let proj = match store.create_projection(sym) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let sliced = slice_code(&proj.text, &[], sym.line_start);
                    bundle_output.push_str(&format!(
                        "{} {} {}:{}\n",
                        sym.name, sym.kind, sym.path, sym.line_start
                    ));
                    for (j, line) in sliced.code.lines().enumerate() {
                        bundle_output.push_str(&format!(" {:>4}: {}\n", sym.line_start + j, line));
                    }
                    bundle_output.push('\n');
                }
            }

            if !bundle_output.is_empty() {
                output.push_str("--- BUNDLED ---\n");
                output.push_str(&bundle_output);
            }
        }

        // Persist cache to disk (no-op if nothing changed)
        {
            let anchor_root = self.root.join(ANCHOR_DIR);
            let mut cache = self.persistent_cache.lock().unwrap();
            cache.save(&anchor_root);
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Search for symbols by name or concept. Returns NAME KIND FILE:LINE. Use before calling context."
    )]
    async fn search(
        &self,
        Parameters(req): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = &self.store;
        let limit = req.limit.unwrap_or(20);
        let results = store
            .search_symbols_hybrid(&req.query, limit)
            .map_err(|e| Self::err(e.to_string()))?;

        let mut output = String::new();
        if results.is_empty() {
            output.push_str(&format!("No symbols match '{}'\n", req.query));
        } else {
            for sym in &results {
                let fname = std::path::Path::new(&sym.path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| sym.path.clone());
                output.push_str(&format!(
                    "{} {} {}:{}\n",
                    sym.name, sym.kind, fname, sym.line_start
                ));
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Get codebase map: modules, entry points, top connected symbols. Use for understanding project structure. Optional scope to zoom into a module."
    )]
    async fn map(
        &self,
        Parameters(req): Parameters<MapRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = &self.store;
        let index = store
            .load_symbol_index()
            .map_err(|e| Self::err(e.to_string()))?;
        let call_index = store.load_call_index();

        use std::collections::{BTreeMap, HashMap, HashSet};

        // Precompute caller counts for all symbols
        let mut caller_count: HashMap<String, usize> = HashMap::new();
        for callees in call_index.calls.values() {
            for callee in callees {
                *caller_count.entry(callee.clone()).or_insert(0) += 1;
            }
        }

        let mut modules: BTreeMap<String, Vec<(String, String, usize, usize)>> = BTreeMap::new();
        let mut all_symbols: Vec<(String, String, usize, usize, String)> = Vec::new();

        for sym in &index.symbols {
            let dir = std::path::Path::new(&sym.path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());

            if let Some(ref s) = req.scope {
                if !dir.contains(s) && !sym.path.contains(s) {
                    continue;
                }
            }

            let callers = *caller_count.get(&sym.name).unwrap_or(&0);
            let callees = call_index
                .calls
                .get(&sym.name)
                .map(|v| v.len())
                .unwrap_or(0);
            let short_module = dir.split('/').next_back().unwrap_or(&dir).to_string();

            modules.entry(dir.clone()).or_default().push((
                sym.name.clone(),
                sym.kind.clone(),
                callers,
                callees,
            ));
            all_symbols.push((
                sym.name.clone(),
                sym.kind.clone(),
                callers,
                callees,
                short_module,
            ));
        }

        let mut output = String::new();

        if modules.is_empty() {
            output.push_str("No symbols found\n");
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Scoped view: show each symbol with its callers/callees
        if req.scope.is_some() {
            for (dir, symbols) in &modules {
                output.push_str(&format!("@{}\n", dir));
                for (name, kind, callers, callees) in symbols {
                    let mut parts = Vec::new();
                    if *callees > 0 {
                        let deps: Vec<&str> =
                            call_index.callees_of(name).into_iter().take(5).collect();
                        if !deps.is_empty() {
                            parts.push(format!(">{}", deps.join(",")));
                        }
                    }
                    if *callers > 0 {
                        let crs: Vec<&str> =
                            call_index.callers_of(name).into_iter().take(5).collect();
                        if !crs.is_empty() {
                            parts.push(format!("<{}", crs.join(",")));
                        }
                    }
                    let short = short_kind(kind);
                    if parts.is_empty() {
                        output.push_str(&format!("  {}.{}\n", name, short));
                    } else {
                        output.push_str(&format!("  {}.{} {}\n", name, short, parts.join(" ")));
                    }
                }
            }
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Top-level view
        let module_line: Vec<String> = modules
            .iter()
            .map(|(dir, symbols)| {
                let short_dir = dir.split('/').next_back().unwrap_or(dir);
                format!("{}({}s)", short_dir, symbols.len())
            })
            .collect();
        output.push_str(&module_line.join(" "));
        output.push('\n');

        // Entry points: called by nobody, calls something, not a test
        let entries: Vec<String> = all_symbols
            .iter()
            .filter(|(name, kind, callers, callees, _)| {
                *callers == 0
                    && *callees > 0
                    && (kind == "Function" || kind == "Method")
                    && !name.starts_with("test_")
                    && name != "new"
            })
            .map(|(name, _, _, _, module)| format!("{}:{}", module, name))
            .take(10)
            .collect();

        if !entries.is_empty() {
            output.push_str(&format!("ENTRY: {}\n", entries.join(" ")));
        }

        // Top connected symbols
        let mut by_connections = all_symbols.clone();
        by_connections.sort_by(|a, b| (b.2 + b.3).cmp(&(a.2 + a.3)));

        let mut seen: HashSet<String> = HashSet::new();
        let mut top: Vec<String> = Vec::new();

        for (name, kind, callers, callees, module) in by_connections.iter() {
            if kind == "Import" || name == "new" {
                continue;
            }
            if seen.insert(name.clone()) {
                top.push(format!("{}:{}({})", module, name, callers + callees));
                if top.len() >= 10 {
                    break;
                }
            }
        }

        if !top.is_empty() {
            output.push_str(&format!("TOP: {}\n", top.join(" ")));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Analyze impact of changing a symbol: callers that break, call chain depth. Use before modifying any function/method to understand blast radius."
    )]
    async fn impact(
        &self,
        Parameters(req): Parameters<ImpactRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = &self.store;
        let candidates = store
            .search_symbols_hybrid(&req.symbol, 1)
            .map_err(|e| Self::err(e.to_string()))?;

        let mut output = String::new();

        if candidates.is_empty() {
            output.push_str(&format!("Symbol '{}' not found\n", req.symbol));
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        let sym = &candidates[0];
        output.push_str(&format!(
            "{} {} {}:{}\n",
            sym.name, sym.kind, sym.path, sym.line_start
        ));

        let call_index = store.load_call_index();
        let callers = call_index.callers_of(&sym.name);

        if callers.is_empty() {
            output.push_str("\nBREAKS: nothing (no callers)\n");
        } else {
            let index = store
                .load_symbol_index()
                .map_err(|e| Self::err(e.to_string()))?;
            output.push_str(&format!("\nBREAKS ({} direct callers):\n", callers.len()));
            for caller_name in callers.iter().take(10) {
                if let Some(s) = index.symbols.iter().find(|s| &s.name == caller_name) {
                    output.push_str(&format!("  {} {}:{}\n", s.name, s.path, s.line_start));
                } else {
                    output.push_str(&format!("  {}\n", caller_name));
                }
            }
            if callers.len() > 10 {
                output.push_str(&format!("  ... and {} more\n", callers.len() - 10));
            }
        }

        let callees = call_index.callees_of(&sym.name);
        if !callees.is_empty() {
            output.push_str(&format!(
                "\nCALLS: {}\n",
                callees
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Unified write tool. mode='range' replaces a line range with symbol-lock safety. mode='ordered' writes multiple files."
    )]
    async fn write(
        &self,
        Parameters(req): Parameters<WriteRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = &self.store;
        let mode_lower = req.mode.trim().to_ascii_lowercase();
        let mode = match mode_lower.as_str() {
            "range" => "range",
            "ordered" => "ordered",
            other => {
                return Err(Self::err(format!(
                    "Invalid write mode '{}'. Use 'range' or 'ordered'.",
                    other
                )));
            }
        };

        if mode == "ordered" {
            let operations = req
                .operations
                .as_ref()
                .ok_or_else(|| Self::err("write mode 'ordered' requires 'operations'"))?;
            if operations.is_empty() {
                return Err(Self::err(
                    "write mode 'ordered' requires at least one operation",
                ));
            }

            let mut output = String::new();
            output.push_str("<ordered_write>\n");
            let start = std::time::Instant::now();

            for (i, op) in operations.iter().enumerate() {
                let full_path = self.root.join(&op.path);
                if let Some(parent) = full_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let result = crate::write::create_file(&full_path, &op.content)
                    .map_err(|e| Self::err(e.to_string()))?;
                // Re-index after write
                let _ = store.upsert_symbols_for_path(&full_path);
                output.push_str(&format!(
                    "  {}. {} ({} lines)\n",
                    i + 1,
                    op.path,
                    result.lines_written
                ));
            }

            output.push_str(&format!(
                "<total_time_ms>{}</total_time_ms>\n",
                start.elapsed().as_millis()
            ));
            output.push_str("</ordered_write>\n");
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // mode == "range"
        let path = req
            .path
            .as_deref()
            .ok_or_else(|| Self::err("write mode 'range' requires 'path'"))?;
        let start_line = req
            .start_line
            .ok_or_else(|| Self::err("write mode 'range' requires 'start_line'"))?;
        let end_line = req
            .end_line
            .ok_or_else(|| Self::err("write mode 'range' requires 'end_line'"))?;
        let new_content = req
            .new_content
            .as_deref()
            .ok_or_else(|| Self::err("write mode 'range' requires 'new_content'"))?;

        let full_path = self.root.join(path);
        if !full_path.exists() {
            return Err(Self::err(format!("File not found: {}", path)));
        }

        // Find which symbols overlap the edit range using the index
        let index = store
            .load_symbol_index()
            .map_err(|e| Self::err(e.to_string()))?;
        let repo_rel = full_path
            .strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string());
        let affected = store.symbols_in_range(&index, &repo_rel, start_line, end_line);
        let affected_names: Vec<String> = affected.iter().map(|s| s.name.clone()).collect();

        // Acquire symbol-level locks before writing.
        // Try lockd (multi-agent daemon) first; fall back to in-process if unavailable.
        // lockd_locked tracks symbols held by lockd; locked_symbols tracks in-process only.
        let use_lockd = lockd::is_available();
        let mut locked_symbols: Vec<SymbolKey> = Vec::new();
        let mut lockd_locked: Vec<(String, String)> = Vec::new();

        for name in &affected_names {
            let key = SymbolKey::new(&full_path, name.as_str());
            let path_str = full_path.to_string_lossy().to_string();

            if use_lockd {
                match lockd::acquire(name, &path_str) {
                    lockd::LockdResult::Acquired => {
                        lockd_locked.push((name.clone(), path_str));
                    }
                    lockd::LockdResult::Blocked { owner, reason } => {
                        for (s, p) in &lockd_locked {
                            lockd::release(s, p);
                        }
                        return Err(Self::err(format!("BLOCKED by {}: {}", owner, reason)));
                    }
                    lockd::LockdResult::Unavailable => {
                        // lockd went away mid-loop — fall through to in-process
                        match self.lock_manager.try_acquire_symbol_simple(&key) {
                            LockResult::Acquired { symbol, .. }
                            | LockResult::AcquiredAfterWait { symbol, .. } => {
                                locked_symbols.push(symbol);
                            }
                            LockResult::Blocked { reason, .. } => {
                                for (s, p) in &lockd_locked {
                                    lockd::release(s, p);
                                }
                                for s in &locked_symbols {
                                    self.lock_manager.release_symbol(s);
                                }
                                return Err(Self::err(format!("BLOCKED: {}", reason)));
                            }
                        }
                    }
                }
            } else {
                match self.lock_manager.try_acquire_symbol_simple(&key) {
                    LockResult::Acquired { symbol, .. }
                    | LockResult::AcquiredAfterWait { symbol, .. } => locked_symbols.push(symbol),
                    LockResult::Blocked { reason, .. } => {
                        for s in &locked_symbols {
                            self.lock_manager.release_symbol(s);
                        }
                        return Err(Self::err(format!("BLOCKED: {}", reason)));
                    }
                }
            }
        }

        let mut output = String::new();

        // Show caller impact before writing
        if !affected.is_empty() {
            let call_index = store.load_call_index();
            output.push_str(&format!("IMPACT: {}:{}-{}\n", path, start_line, end_line));
            for sym in &affected {
                let callers = call_index.callers_of(&sym.name);
                if !callers.is_empty() {
                    output.push_str(&format!(
                        "  {} — {} callers affected\n",
                        sym.name,
                        callers.len()
                    ));
                    for c in callers.iter().take(5) {
                        output.push_str(&format!("    > {}\n", c));
                    }
                }
            }
            output.push('\n');
        }

        let result = crate::write::replace_range(&full_path, start_line, end_line, new_content)
            .map_err(|e| {
                for (s, p) in &lockd_locked {
                    lockd::release(s, p);
                }
                for s in &locked_symbols {
                    self.lock_manager.release_symbol(s);
                }
                Self::err(e.to_string())
            })?;

        // Re-index the changed file
        let _ = store.upsert_symbols_for_path(&full_path);

        // Release all locks
        for (s, p) in &lockd_locked {
            lockd::release(s, p);
        }
        for s in &locked_symbols {
            self.lock_manager.release_symbol(s);
        }

        output.push_str(&format!(
            "WRITTEN: {}:{}-{} ({} lines)\n",
            path, start_line, end_line, result.lines_written
        ));

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

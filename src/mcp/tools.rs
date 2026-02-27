//
//  tools.rs
//  Anchor
//
//  Created by hak (tharun)
//

use rmcp::{handler::server::wrapper::Parameters, model::*, tool, tool_router};
use std::path::Path;
use std::sync::Arc;

use super::format::{escape_graphql, format_symbol, short_kind};
use super::types::*;
use super::AnchorMcp;
use crate::graph::{build_graph, rebuild_file, CodeGraph};
use crate::graphql::{build_schema, execute};
use crate::lock::{LockManager, LockResult, SymbolKey};

fn escape_regex_literal(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$'
            | '|' | '&' | '~' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

#[tool_router]
impl AnchorMcp {
    pub fn new(roots: Vec<std::path::PathBuf>) -> Self {
        let root = roots[0].clone();
        let cache_path = root.join(".anchor/graph.bin");
        let root_refs: Vec<&Path> = roots.iter().map(|r| r.as_path()).collect();
        let graph = if cache_path.exists() {
            CodeGraph::load(&cache_path).unwrap_or_else(|_| build_graph(&root_refs))
        } else {
            let graph = build_graph(&root_refs);
            if let Some(parent) = cache_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = graph.save(&cache_path);
            graph
        };

        Self {
            root,
            tool_router: Self::tool_router(),
            graph: Arc::new(std::sync::RwLock::new(graph)),
            lock_manager: Arc::new(LockManager::new()),
        }
    }

    fn load_graph(&self) -> Result<Arc<CodeGraph>, ErrorData> {
        let guard = self
            .graph
            .read()
            .map_err(|e| Self::err(format!("Graph lock poisoned: {}", e)))?;
        Ok(Arc::new(guard.clone()))
    }

    fn err(msg: impl Into<String>) -> ErrorData {
        ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: std::borrow::Cow::from(msg.into()),
            data: None,
        }
    }

    #[tool(
        description = "Get full context for symbols: sliced code + callers + callees. Returns exact line numbers you can pass directly to 'write'. Supports multiple symbols in one call. Shows line coverage (e.g. [25/88 lines, 3 calls]) when sliced. Set full=true to disable slicing."
    )]
    async fn context(
        &self,
        Parameters(req): Parameters<ContextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let graph = self.load_graph()?;
        let schema = build_schema(graph);
        let limit = req.limit.unwrap_or(5);
        let full = req.full.unwrap_or(false);

        let mut output = String::new();

        for (i, query) in req.symbols.iter().enumerate() {
            if i > 0 {
                output.push_str("\n===\n");
            }

            let gql_query = format!(
                r#"{{ symbol(name: "{}") {{ name kind file line code(full: {}) callers {{ name }} callees {{ name }} }} }}"#,
                escape_graphql(query),
                full,
            );

            let result = execute(&schema, &gql_query).await;
            let json: serde_json::Value = serde_json::from_str(&result)
                .map_err(|e| Self::err(format!("JSON parse error: {}", e)))?;

            let symbols = json
                .get("data")
                .and_then(|d| d.get("symbol"))
                .and_then(|s| s.as_array());

            match symbols {
                Some(syms) if !syms.is_empty() => {
                    for sym in syms.iter().take(limit) {
                        format_symbol(&mut output, sym);
                    }
                }
                _ => {
                    output.push_str(&format!("No results for '{}'\n", query));
                }
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Search for symbols by name or regex pattern. Returns lightweight results: NAME KIND FILE:LINE. Use for finding symbols before calling context."
    )]
    async fn search(
        &self,
        Parameters(req): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let graph = self.load_graph()?;
        let schema = build_schema(graph);
        let limit = req.limit.unwrap_or(20);

        let gql_query = if let Some(pat) = &req.pattern {
            format!(
                r#"{{ search(pattern: "{}", limit: {}) {{ name kind file line }} }}"#,
                escape_graphql(pat),
                limit
            )
        } else {
            let escaped = escape_regex_literal(&req.query.to_lowercase());
            let regex_pat = format!(".*{}.*", escaped);
            format!(
                r#"{{ search(pattern: "{}", limit: {}) {{ name kind file line }} }}"#,
                escape_graphql(&regex_pat),
                limit
            )
        };

        let result = execute(&schema, &gql_query).await;
        let json: serde_json::Value = serde_json::from_str(&result)
            .map_err(|e| Self::err(format!("JSON parse error: {}", e)))?;

        let mut output = String::new();

        if let Some(symbols) = json
            .get("data")
            .and_then(|d| d.get("search"))
            .and_then(|s| s.as_array())
        {
            if symbols.is_empty() {
                output.push_str(&format!("No symbols match '{}'\n", req.query));
            } else {
                for sym in symbols.iter().take(limit) {
                    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                    let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
                    let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);
                    let file_name = Path::new(file)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.to_string());
                    output.push_str(&format!("{} {} {}:{}\n", name, kind, file_name, line));
                }
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
        let graph = self.load_graph()?;

        use std::collections::{BTreeMap, HashSet};

        let mut modules: BTreeMap<String, Vec<(String, String, usize, usize)>> = BTreeMap::new();
        let mut all_symbols: Vec<(String, String, usize, usize, String)> = Vec::new();

        for file_path in graph.all_files() {
            let dir = file_path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());

            if let Some(ref s) = req.scope {
                if !dir.contains(s) && !file_path.to_string_lossy().contains(s) {
                    continue;
                }
            }

            for symbol in graph.symbols_in_file(&file_path) {
                if matches!(
                    symbol.kind,
                    crate::graph::types::NodeKind::Import | crate::graph::types::NodeKind::File
                ) {
                    continue;
                }

                let callers = graph.dependents(&symbol.name).len();
                let callees = graph.dependencies(&symbol.name).len();
                let short_module = dir.split('/').next_back().unwrap_or(&dir).to_string();

                modules.entry(dir.clone()).or_default().push((
                    symbol.name.clone(),
                    symbol.kind.to_string(),
                    callers,
                    callees,
                ));

                all_symbols.push((
                    symbol.name.clone(),
                    symbol.kind.to_string(),
                    callers,
                    callees,
                    short_module,
                ));
            }
        }

        let mut output = String::new();

        if modules.is_empty() {
            output.push_str("No symbols found\n");
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        // Scoped view
        if req.scope.is_some() {
            for (dir, symbols) in &modules {
                output.push_str(&format!("@{}\n", dir));
                for (name, kind, callers, callees) in symbols {
                    let mut parts = Vec::new();
                    if *callees > 0 {
                        let deps: Vec<String> = graph
                            .dependencies(name)
                            .iter()
                            .take(5)
                            .map(|d| d.symbol.clone())
                            .collect();
                        if !deps.is_empty() {
                            parts.push(format!(">{}", deps.join(",")));
                        }
                    }
                    if *callers > 0 {
                        let callers_list: Vec<String> = graph
                            .dependents(name)
                            .iter()
                            .take(5)
                            .map(|d| d.symbol.clone())
                            .collect();
                        if !callers_list.is_empty() {
                            parts.push(format!("<{}", callers_list.join(",")));
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

        // Entry points
        let entries: Vec<String> = all_symbols
            .iter()
            .filter(|(name, kind, callers, callees, _)| {
                *callers == 0
                    && *callees > 0
                    && (kind == "function" || kind == "method")
                    && !name.starts_with("test_")
                    && name != "new"
            })
            .map(|(name, _, _, _, module)| format!("{}:{}", module, name))
            .take(10)
            .collect();

        if !entries.is_empty() {
            output.push_str(&format!("ENTRY: {}\n", entries.join(" ")));
        }

        // Top connected
        let mut by_connections = all_symbols.clone();
        by_connections.sort_by(|a, b| (b.2 + b.3).cmp(&(a.2 + a.3)));

        let mut seen: HashSet<String> = HashSet::new();
        let mut top: Vec<String> = Vec::new();

        for (name, kind, callers, callees, module) in by_connections.iter() {
            if kind == "import" || name == "new" {
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
        description = "Analyze impact of changing a symbol: what breaks, suggested fixes, affected tests. Use before modifying any function/method to understand blast radius."
    )]
    async fn impact(
        &self,
        Parameters(req): Parameters<ImpactRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let graph = self.load_graph()?;
        let response = crate::query::get_context_for_change(
            &graph,
            &req.symbol,
            "change",
            req.new_signature.as_deref(),
        );

        let mut output = String::new();

        if !response.found {
            output.push_str(&format!("Symbol '{}' not found\n", req.symbol));
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        if let Some(sym) = response.symbols.first() {
            output.push_str(&format!(
                "{} {} {}:{}\n",
                sym.name, sym.kind, sym.file, sym.line
            ));
        }

        if !response.used_by.is_empty() {
            output.push_str(&format!("\nBREAKS ({} callers):\n", response.used_by.len()));
            for r in response.used_by.iter().take(5) {
                output.push_str(&format!("  {} in {}:{}\n", r.name, r.file, r.line));
            }
            if response.used_by.len() > 5 {
                output.push_str(&format!("  ... and {} more\n", response.used_by.len() - 5));
            }
        } else {
            output.push_str("\nBREAKS: nothing (no callers)\n");
        }
        if !response.edits.is_empty() {
            output.push_str(&format!(
                "\nEDITS ({} changes needed):\n",
                response.edits.len()
            ));
            for edit in &response.edits {
                output.push_str(&format!(
                    "  {}:{} in {}\n",
                    edit.file, edit.line, edit.in_symbol
                ));
                output.push_str(&format!("    now: {}\n", edit.usage));
                if let Some(ref suggested) = edit.suggested {
                    output.push_str(&format!("    fix: {}\n", suggested));
                }
                if !edit.new_args.is_empty() {
                    output.push_str(&format!("    +args: {}\n", edit.new_args.join(", ")));
                }
                if !edit.removed_args.is_empty() {
                    output.push_str(&format!("    -args: {}\n", edit.removed_args.join(", ")));
                }
            }
        }

        if !response.tests.is_empty() {
            output.push_str(&format!("\nTESTS ({} to update):\n", response.tests.len()));
            for test in &response.tests {
                output.push_str(&format!("  {} {}:{}\n", test.name, test.file, test.line));
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "Unified write tool. mode='range' replaces a line range with impact analysis. mode='ordered' writes multiple files in graph dependency order."
    )]
    async fn write(
        &self,
        Parameters(req): Parameters<WriteRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let graph = self.load_graph()?;
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

            let ops: Vec<crate::write::WriteOp> = operations
                .iter()
                .map(|op| crate::write::WriteOp {
                    path: self.root.join(&op.path),
                    content: op.content.clone(),
                    symbol: op.symbol.clone(),
                })
                .collect();

            let result =
                crate::write::write_ordered(&graph, &ops).map_err(|e| Self::err(e.to_string()))?;

            // Re-index each written file so the graph stays in sync
            if let Ok(mut graph_mut) = self.graph.write() {
                for op in &ops {
                    let _ = rebuild_file(&mut graph_mut, &op.path);
                }
            }

            let mut output = String::new();
            output.push_str("<ordered_write>\n");
            output.push_str(&format!(
                "<total_operations>{}</total_operations>\n",
                result.total_operations
            ));
            output.push_str("<write_order>\n");
            for (i, path) in result.write_order.iter().enumerate() {
                output.push_str(&format!("  {}. {}\n", i + 1, path));
            }
            output.push_str("</write_order>\n");
            output.push_str(&format!(
                "<total_time_ms>{}</total_time_ms>\n",
                result.total_time_ms
            ));
            output.push_str("<results>\n");
            for r in &result.results {
                output.push_str(&format!(
                    "  <file path=\"{}\" lines=\"{}\" bytes=\"{}\"/>\n",
                    r.path, r.lines_written, r.bytes_written
                ));
            }
            output.push_str("</results>\n");
            output.push_str("</ordered_write>\n");

            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

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

        let mut output = String::new();
        let affected = graph.symbols_in_range(&full_path, start_line, end_line);
        let affected_names: Vec<String> = affected.iter().map(|s| s.name.clone()).collect();

        // Lock affected symbols before writing
        let mut locked_symbols = Vec::new();
        {
            let graph_ref = self
                .graph
                .read()
                .map_err(|e| Self::err(format!("Graph lock poisoned: {}", e)))?;
            for name in &affected_names {
                let key = SymbolKey::new(&full_path, name.as_str());
                match self.lock_manager.try_acquire_symbol(&key, &graph_ref) {
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

        if !affected.is_empty() {
            output.push_str(&format!("IMPACT: {}:{}-{}\n", path, start_line, end_line));

            for sym in &affected {
                let response =
                    crate::query::get_context_for_change(&graph, &sym.name, "change", None);

                if !response.used_by.is_empty() {
                    output.push_str(&format!(
                        "  {} — {} callers affected\n",
                        sym.name,
                        response.used_by.len()
                    ));
                    for r in response.used_by.iter().take(5) {
                        output.push_str(&format!("    > {} in {}:{}\n", r.name, r.file, r.line));
                    }
                    if response.used_by.len() > 5 {
                        output.push_str(&format!(
                            "    ... and {} more\n",
                            response.used_by.len() - 5
                        ));
                    }
                }

                if !response.tests.is_empty() {
                    output.push_str(&format!(
                        "  tests: {}\n",
                        response
                            .tests
                            .iter()
                            .map(|t| t.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }

            output.push('\n');
        }

        let result = crate::write::replace_range(&full_path, start_line, end_line, new_content)
            .map_err(|e| {
                // Release locks on write failure
                for s in &locked_symbols {
                    self.lock_manager.release_symbol(s);
                }
                Self::err(e.to_string())
            })?;

        // Re-index the changed file so the graph stays in sync
        if let Ok(mut graph_mut) = self.graph.write() {
            let _ = rebuild_file(&mut graph_mut, &full_path);
        }

        // Release all locks after write + rebuild
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

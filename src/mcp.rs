//! MCP (Model Context Protocol) server for Anchor.
//!
//! Exposes Anchor's code intelligence as native MCP tools.
//! Agents connect via stdio and get: context, search, map.

use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::*,
    schemars, tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::graph::{build_graph, CodeGraph};
use crate::graphql::{build_schema, execute};

// ─── MCP Server ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AnchorMcp {
    root: PathBuf,
    tool_router: ToolRouter<AnchorMcp>,
}

// ─── Tool Input Types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ContextRequest {
    #[schemars(description = "Symbol names to get context for (e.g. [\"login\", \"UserService\"])")]
    pub symbols: Vec<String>,

    #[schemars(description = "Max results per symbol (default: 5)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    #[schemars(description = "Symbol name to search for")]
    pub query: String,

    #[schemars(description = "Regex pattern for advanced search (Brzozowski derivatives, ReDoS-safe)")]
    pub pattern: Option<String>,

    #[schemars(description = "Max results (default: 20)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapRequest {
    #[schemars(description = "Optional scope to zoom into (e.g. \"src/graph\" or \"auth\")")]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImpactRequest {
    #[schemars(description = "Symbol name to analyze impact for (e.g. \"login\", \"UserService\")")]
    pub symbol: String,

    #[schemars(description = "Optional new signature if you're changing the function (e.g. \"fn login(user: &str, token: &str) -> Result<bool>\")")]
    pub new_signature: Option<String>,
}

// ─── Tool Implementations ────────────────────────────────────────────────────

#[tool_router]
impl AnchorMcp {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            tool_router: Self::tool_router(),
        }
    }

    /// Load or build the graph
    fn load_graph(&self) -> Result<CodeGraph, ErrorData> {
        let cache_path = self.root.join(".anchor/graph.bin");

        if cache_path.exists() {
            if let Ok(graph) = CodeGraph::load(&cache_path) {
                return Ok(graph);
            }
        }

        // Auto-build if no cache
        let graph = build_graph(&self.root);
        if let Some(parent) = cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = graph.save(&cache_path);
        Ok(graph)
    }

    fn err(msg: impl Into<String>) -> ErrorData {
        ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from(msg.into()),
            data: None,
        }
    }

    #[tool(description = "Get full context for symbols: sliced code + callers + callees. Use this for understanding code. Supports multiple symbols in one call.")]
    async fn context(
        &self,
        Parameters(req): Parameters<ContextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let graph = self.load_graph()?;
        let schema = build_schema(Arc::new(graph));
        let limit = req.limit.unwrap_or(5);

        let mut output = String::new();

        for (i, query) in req.symbols.iter().enumerate() {
            if i > 0 {
                output.push_str("\n===\n");
            }

            let gql_query = format!(
                r#"{{ symbol(name: "{}") {{ name kind file line code callers {{ name }} callees {{ name }} }} }}"#,
                escape_graphql(query)
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

    #[tool(description = "Search for symbols by name or regex pattern. Returns lightweight results: NAME KIND FILE:LINE. Use for finding symbols before calling context.")]
    async fn search(
        &self,
        Parameters(req): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let graph = self.load_graph()?;
        let schema = build_schema(Arc::new(graph));
        let limit = req.limit.unwrap_or(20);

        let gql_query = if let Some(pat) = &req.pattern {
            format!(
                r#"{{ search(pattern: "{}", limit: {}) {{ name kind file line }} }}"#,
                escape_graphql(pat),
                limit
            )
        } else {
            let regex_pat = format!(".*{}.*", req.query).to_lowercase();
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

        if let Some(symbols) = json.get("data").and_then(|d| d.get("search")).and_then(|s| s.as_array()) {
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

    #[tool(description = "Get codebase map: modules, entry points, top connected symbols. Use for understanding project structure. Optional scope to zoom into a module.")]
    async fn map(
        &self,
        Parameters(req): Parameters<MapRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let graph = self.load_graph()?;

        use std::collections::{BTreeMap, HashSet};

        let mut modules: BTreeMap<String, Vec<(String, String, usize, usize)>> = BTreeMap::new();
        let mut all_symbols: Vec<(String, String, usize, usize, String)> = Vec::new();

        for file_path in graph.all_files() {
            let dir = file_path.parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());

            if let Some(ref s) = req.scope {
                if !dir.contains(s) && !file_path.to_string_lossy().contains(s) {
                    continue;
                }
            }

            for symbol in graph.symbols_in_file(&file_path) {
                if matches!(symbol.kind, crate::graph::types::NodeKind::Import | crate::graph::types::NodeKind::File) {
                    continue;
                }

                let callers = graph.dependents(&symbol.name).len();
                let callees = graph.dependencies(&symbol.name).len();
                let short_module = dir.split('/').last().unwrap_or(&dir).to_string();

                modules.entry(dir.clone())
                    .or_default()
                    .push((symbol.name.clone(), symbol.kind.to_string(), callers, callees));

                all_symbols.push((symbol.name.clone(), symbol.kind.to_string(), callers, callees, short_module));
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
                        let deps: Vec<String> = graph.dependencies(name)
                            .iter().take(5).map(|d| d.symbol.clone()).collect();
                        if !deps.is_empty() {
                            parts.push(format!(">{}", deps.join(",")));
                        }
                    }
                    if *callers > 0 {
                        let callers_list: Vec<String> = graph.dependents(name)
                            .iter().take(5).map(|d| d.symbol.clone()).collect();
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
        let module_line: Vec<String> = modules.iter()
            .map(|(dir, symbols)| {
                let short_dir = dir.split('/').last().unwrap_or(dir);
                format!("{}({}s)", short_dir, symbols.len())
            })
            .collect();
        output.push_str(&module_line.join(" "));
        output.push('\n');

        // Entry points
        let entries: Vec<String> = all_symbols.iter()
            .filter(|(name, kind, callers, callees, _)| {
                *callers == 0 && *callees > 0 &&
                (kind == "function" || kind == "method") &&
                !name.starts_with("test_") && name != "new"
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
            if kind == "import" || name == "new" { continue; }
            if seen.insert(name.clone()) {
                top.push(format!("{}:{}({})", module, name, callers + callees));
                if top.len() >= 10 { break; }
            }
        }

        if !top.is_empty() {
            output.push_str(&format!("TOP: {}\n", top.join(" ")));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(description = "Analyze impact of changing a symbol: what breaks, suggested fixes, affected tests. Use before modifying any function/method to understand blast radius.")]
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

        // Symbol info
        if let Some(sym) = response.symbols.first() {
            output.push_str(&format!("{} {} {}:{}\n", sym.name, sym.kind, sym.file, sym.line));
        }

        // Who uses this (callers that would break)
        if !response.used_by.is_empty() {
            output.push_str(&format!("\nBREAKS ({} callers):\n", response.used_by.len()));
            for r in &response.used_by {
                output.push_str(&format!("  {} in {}:{}\n", r.name, r.file, r.line));
            }
        } else {
            output.push_str("\nBREAKS: nothing (no callers)\n");
        }

        // Suggested edits
        if !response.edits.is_empty() {
            output.push_str(&format!("\nEDITS ({} changes needed):\n", response.edits.len()));
            for edit in &response.edits {
                output.push_str(&format!("  {}:{} in {}\n", edit.file, edit.line, edit.in_symbol));
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

        // Related tests
        if !response.tests.is_empty() {
            output.push_str(&format!("\nTESTS ({} to update):\n", response.tests.len()));
            for test in &response.tests {
                output.push_str(&format!("  {} {}:{}\n", test.name, test.file, test.line));
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

// ─── ServerHandler ───────────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for AnchorMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "anchor".into(),
                version: crate::updater::VERSION.into(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Anchor: Code intelligence for AI agents. Use 'context' for symbol lookup (code + callers + callees, graph-sliced). Use 'search' to find symbols. Use 'map' for codebase overview. Use 'impact' before changing any symbol to see what breaks. Context handles multiple symbols in one call.".into()
            ),
        }
    }
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

/// Run the MCP server on stdio
pub async fn run(root: PathBuf) -> anyhow::Result<()> {
    let service = AnchorMcp::new(root);
    let server = service.serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn escape_graphql(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn format_symbol(output: &mut String, sym: &serde_json::Value) {
    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let kind = sym.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let file = sym.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let line = sym.get("line").and_then(|v| v.as_i64()).unwrap_or(0);

    let file_name = Path::new(file)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| file.to_string());

    output.push_str(&format!("{} {} {}:{}\n", name, kind, file_name, line));

    // Callers
    if let Some(callers) = sym.get("callers").and_then(|c| c.as_array()) {
        let mut names: Vec<&str> = callers.iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .filter(|n| !is_file_name(n))
            .collect();
        names.sort();
        names.dedup();
        if !names.is_empty() {
            output.push_str(&format!("> {}\n", names.join(" ")));
        }
    }

    // Callees
    if let Some(callees) = sym.get("callees").and_then(|c| c.as_array()) {
        let mut names: Vec<&str> = callees.iter()
            .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
            .filter(|n| !is_file_name(n))
            .collect();
        names.sort();
        names.dedup();
        if !names.is_empty() {
            output.push_str(&format!("< {}\n", names.join(" ")));
        }
    }

    // Code
    if let Some(code) = sym.get("code").and_then(|c| c.as_str()) {
        output.push_str("---\n");
        output.push_str(code);
        output.push('\n');
    }
}

fn is_file_name(s: &str) -> bool {
    s.ends_with(".rs") || s.ends_with(".py") || s.ends_with(".js") || s.ends_with(".ts")
}

fn short_kind(kind: &str) -> &str {
    match kind {
        "function" => "fn",
        "method" => "m",
        "struct" => "st",
        "class" => "cl",
        "trait" => "tr",
        "interface" => "if",
        "enum" => "en",
        "constant" => "c",
        "module" => "mod",
        "type" => "ty",
        "variable" => "v",
        "impl" => "impl",
        _ => kind,
    }
}

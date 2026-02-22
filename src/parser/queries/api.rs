//
//  api.rs
//  Anchor
//
//  Unified API endpoint extractor. One generic walker + pattern tables per language.
//  Replaces all per-language extractors (python.rs, javascript.rs, go.rs, etc.)
//

use std::path::Path;
use tree_sitter::Node;

use crate::graph::types::{ApiEndpointKind, ExtractedApiEndpoint};
use crate::parser::language::SupportedLanguage;

// ── Pattern Types ────────────────────────────────────────────────────────────

/// A text pattern that identifies an API endpoint in source code.
struct ApiPattern {
    /// Text to search for in node content
    text: &'static str,
    /// HTTP method (None = auto-detect from text)
    method: Option<&'static str>,
    /// true = server route (Defines), false = client call (Consumes)
    is_server: bool,
    /// Only match if file is a backend file (JS/TS only)
    backend_only: bool,
    /// Only match on these node kinds (empty = match on any check_node)
    only_on: &'static [&'static str],
}

impl ApiPattern {
    const fn server(text: &'static str, method: Option<&'static str>) -> Self {
        Self {
            text,
            method,
            is_server: true,
            backend_only: false,
            only_on: &[],
        }
    }
    const fn client(text: &'static str, method: Option<&'static str>) -> Self {
        Self {
            text,
            method,
            is_server: false,
            backend_only: false,
            only_on: &[],
        }
    }
    const fn server_on(
        text: &'static str,
        method: Option<&'static str>,
        only_on: &'static [&'static str],
    ) -> Self {
        Self {
            text,
            method,
            is_server: true,
            backend_only: false,
            only_on,
        }
    }
    const fn client_on(
        text: &'static str,
        method: Option<&'static str>,
        only_on: &'static [&'static str],
    ) -> Self {
        Self {
            text,
            method,
            is_server: false,
            backend_only: false,
            only_on,
        }
    }
    const fn server_backend(text: &'static str, method: Option<&'static str>) -> Self {
        Self {
            text,
            method,
            is_server: true,
            backend_only: true,
            only_on: &[],
        }
    }
}

/// Per-language configuration for the generic walker.
struct LangApiConfig {
    /// Node kinds to inspect for API patterns
    check_nodes: &'static [&'static str],
    /// Node kinds that define function/method scope
    fn_scope: &'static [&'static str],
    /// Node kinds that define class/struct scope
    class_scope: &'static [&'static str],
    /// Text markers on class children that indicate a base path
    base_path_markers: &'static [&'static str],
    /// Ordered list of patterns (first match wins)
    patterns: &'static [ApiPattern],
}

// ── Language Configs ─────────────────────────────────────────────────────────

const PYTHON: LangApiConfig = LangApiConfig {
    check_nodes: &["decorator", "call"],
    fn_scope: &["function_definition"],
    class_scope: &["class_definition"],
    base_path_markers: &[],
    patterns: &[
        // Server: Flask/FastAPI/Sanic decorator patterns
        ApiPattern::server_on(".route(", None, &["decorator"]),
        ApiPattern::server_on(".get(", Some("GET"), &["decorator"]),
        ApiPattern::server_on(".post(", Some("POST"), &["decorator"]),
        ApiPattern::server_on(".put(", Some("PUT"), &["decorator"]),
        ApiPattern::server_on(".delete(", Some("DELETE"), &["decorator"]),
        ApiPattern::server_on(".patch(", Some("PATCH"), &["decorator"]),
        ApiPattern::server_on(".head(", Some("HEAD"), &["decorator"]),
        // Client: requests, httpx, aiohttp, etc.
        ApiPattern::client_on("requests.get(", Some("GET"), &["call"]),
        ApiPattern::client_on("requests.post(", Some("POST"), &["call"]),
        ApiPattern::client_on("requests.put(", Some("PUT"), &["call"]),
        ApiPattern::client_on("requests.delete(", Some("DELETE"), &["call"]),
        ApiPattern::client_on("requests.patch(", Some("PATCH"), &["call"]),
        ApiPattern::client_on("httpx.get(", Some("GET"), &["call"]),
        ApiPattern::client_on("httpx.post(", Some("POST"), &["call"]),
        ApiPattern::client_on("httpx.put(", Some("PUT"), &["call"]),
        ApiPattern::client_on("httpx.delete(", Some("DELETE"), &["call"]),
        ApiPattern::client_on("httpx.patch(", Some("PATCH"), &["call"]),
        ApiPattern::client_on("session.get(", Some("GET"), &["call"]),
        ApiPattern::client_on("session.post(", Some("POST"), &["call"]),
        ApiPattern::client_on("client.get(", Some("GET"), &["call"]),
        ApiPattern::client_on("client.post(", Some("POST"), &["call"]),
    ],
};

const JAVASCRIPT: LangApiConfig = LangApiConfig {
    check_nodes: &["call_expression"],
    fn_scope: &[
        "function_declaration",
        "method_definition",
        "variable_declarator",
    ],
    class_scope: &["class_declaration"],
    base_path_markers: &[],
    patterns: &[
        // Client: fetch, axios, etc.
        ApiPattern::client("fetch(", None),
        ApiPattern::client("axios.get(", Some("GET")),
        ApiPattern::client("axios.post(", Some("POST")),
        ApiPattern::client("axios.put(", Some("PUT")),
        ApiPattern::client("axios.delete(", Some("DELETE")),
        ApiPattern::client("axios.patch(", Some("PATCH")),
        ApiPattern::client("http.get(", Some("GET")),
        ApiPattern::client("http.post(", Some("POST")),
        ApiPattern::client("api.get(", Some("GET")),
        ApiPattern::client("api.post(", Some("POST")),
        ApiPattern::client("ky.get(", Some("GET")),
        ApiPattern::client("ky.post(", Some("POST")),
        ApiPattern::client("got.get(", Some("GET")),
        ApiPattern::client("got.post(", Some("POST")),
        ApiPattern::client("client.get(", Some("GET")),
        ApiPattern::client("client.post(", Some("POST")),
        ApiPattern::client("request.get(", Some("GET")),
        ApiPattern::client("request.post(", Some("POST")),
        // Server: Express/Fastify/Hono/Koa (backend files only)
        ApiPattern::server_backend("app.get(", Some("GET")),
        ApiPattern::server_backend("app.post(", Some("POST")),
        ApiPattern::server_backend("app.put(", Some("PUT")),
        ApiPattern::server_backend("app.delete(", Some("DELETE")),
        ApiPattern::server_backend("app.patch(", Some("PATCH")),
        ApiPattern::server_backend("app.all(", None),
        ApiPattern::server_backend("app.use(", None),
        ApiPattern::server_backend("app.route(", None),
        ApiPattern::server_backend("router.get(", Some("GET")),
        ApiPattern::server_backend("router.post(", Some("POST")),
        ApiPattern::server_backend("router.put(", Some("PUT")),
        ApiPattern::server_backend("router.delete(", Some("DELETE")),
        ApiPattern::server_backend("router.patch(", Some("PATCH")),
        ApiPattern::server_backend("server.get(", Some("GET")),
        ApiPattern::server_backend("server.post(", Some("POST")),
        ApiPattern::server_backend("express.get(", Some("GET")),
        ApiPattern::server_backend("fastify.get(", Some("GET")),
        ApiPattern::server_backend("fastify.post(", Some("POST")),
        ApiPattern::server_backend("hono.get(", Some("GET")),
        ApiPattern::server_backend("hono.post(", Some("POST")),
    ],
};

const GO: LangApiConfig = LangApiConfig {
    check_nodes: &["call_expression"],
    fn_scope: &["function_declaration"],
    class_scope: &[],
    base_path_markers: &[],
    patterns: &[
        // Client: stdlib + libraries (check BEFORE server to avoid ambiguity)
        ApiPattern::client("http.Get(", Some("GET")),
        ApiPattern::client("http.Post(", Some("POST")),
        ApiPattern::client("http.Head(", Some("HEAD")),
        ApiPattern::client("http.PostForm(", Some("POST")),
        ApiPattern::client("http.NewRequest(", None),
        ApiPattern::client("client.Get(", Some("GET")),
        ApiPattern::client("client.Post(", Some("POST")),
        ApiPattern::client("client.Put(", Some("PUT")),
        ApiPattern::client("client.Delete(", Some("DELETE")),
        // Server: Gin/Echo (ALL-CAPS methods)
        ApiPattern::server(".GET(", Some("GET")),
        ApiPattern::server(".POST(", Some("POST")),
        ApiPattern::server(".PUT(", Some("PUT")),
        ApiPattern::server(".DELETE(", Some("DELETE")),
        ApiPattern::server(".PATCH(", Some("PATCH")),
        ApiPattern::server(".HEAD(", Some("HEAD")),
        ApiPattern::server(".OPTIONS(", Some("OPTIONS")),
        ApiPattern::server(".ANY(", None),
        // Server: Chi/Fiber (PascalCase) — client patterns already matched above
        ApiPattern::server(".Get(", Some("GET")),
        ApiPattern::server(".Post(", Some("POST")),
        ApiPattern::server(".Put(", Some("PUT")),
        ApiPattern::server(".Delete(", Some("DELETE")),
        ApiPattern::server(".Patch(", Some("PATCH")),
        ApiPattern::server("HandleFunc(", Some("GET")),
        ApiPattern::server(".Handle(", None),
    ],
};

const JAVA: LangApiConfig = LangApiConfig {
    check_nodes: &["annotation", "marker_annotation", "method_invocation"],
    fn_scope: &["method_declaration"],
    class_scope: &["class_declaration"],
    base_path_markers: &["RequestMapping"],
    patterns: &[
        // Server: Spring annotations
        ApiPattern::server_on(
            "GetMapping",
            Some("GET"),
            &["annotation", "marker_annotation"],
        ),
        ApiPattern::server_on(
            "PostMapping",
            Some("POST"),
            &["annotation", "marker_annotation"],
        ),
        ApiPattern::server_on(
            "PutMapping",
            Some("PUT"),
            &["annotation", "marker_annotation"],
        ),
        ApiPattern::server_on(
            "DeleteMapping",
            Some("DELETE"),
            &["annotation", "marker_annotation"],
        ),
        ApiPattern::server_on(
            "PatchMapping",
            Some("PATCH"),
            &["annotation", "marker_annotation"],
        ),
        ApiPattern::server_on("RequestMapping", None, &["annotation", "marker_annotation"]),
        // Client: RestTemplate, WebClient
        ApiPattern::client_on("getForObject(", Some("GET"), &["method_invocation"]),
        ApiPattern::client_on("getForEntity(", Some("GET"), &["method_invocation"]),
        ApiPattern::client_on("postForObject(", Some("POST"), &["method_invocation"]),
        ApiPattern::client_on("postForEntity(", Some("POST"), &["method_invocation"]),
        ApiPattern::client_on("exchange(", None, &["method_invocation"]),
        ApiPattern::client_on("patchForObject(", Some("PATCH"), &["method_invocation"]),
    ],
};

const CSHARP: LangApiConfig = LangApiConfig {
    check_nodes: &["attribute_list", "invocation_expression"],
    fn_scope: &["method_declaration"],
    class_scope: &["class_declaration"],
    base_path_markers: &["Route("],
    patterns: &[
        // Server: ASP.NET attributes
        ApiPattern::server_on("HttpGet", Some("GET"), &["attribute_list"]),
        ApiPattern::server_on("HttpPost", Some("POST"), &["attribute_list"]),
        ApiPattern::server_on("HttpPut", Some("PUT"), &["attribute_list"]),
        ApiPattern::server_on("HttpDelete", Some("DELETE"), &["attribute_list"]),
        ApiPattern::server_on("HttpPatch", Some("PATCH"), &["attribute_list"]),
        // Server: Minimal API
        ApiPattern::server_on("MapGet(", Some("GET"), &["invocation_expression"]),
        ApiPattern::server_on("MapPost(", Some("POST"), &["invocation_expression"]),
        ApiPattern::server_on("MapPut(", Some("PUT"), &["invocation_expression"]),
        ApiPattern::server_on("MapDelete(", Some("DELETE"), &["invocation_expression"]),
        ApiPattern::server_on("MapPatch(", Some("PATCH"), &["invocation_expression"]),
        // Client: HttpClient
        ApiPattern::client_on("GetAsync(", Some("GET"), &["invocation_expression"]),
        ApiPattern::client_on("PostAsync(", Some("POST"), &["invocation_expression"]),
        ApiPattern::client_on("PutAsync(", Some("PUT"), &["invocation_expression"]),
        ApiPattern::client_on("DeleteAsync(", Some("DELETE"), &["invocation_expression"]),
        ApiPattern::client_on("PatchAsync(", Some("PATCH"), &["invocation_expression"]),
        ApiPattern::client_on("GetStringAsync(", Some("GET"), &["invocation_expression"]),
        ApiPattern::client_on("GetFromJsonAsync(", Some("GET"), &["invocation_expression"]),
        ApiPattern::client_on("PostAsJsonAsync(", Some("POST"), &["invocation_expression"]),
    ],
};

const RUBY: LangApiConfig = LangApiConfig {
    check_nodes: &["call", "method_call"],
    fn_scope: &["method", "singleton_method"],
    class_scope: &["class"],
    base_path_markers: &[],
    patterns: &[
        // Client: HTTP libraries (check before server to avoid .get ambiguity)
        ApiPattern::client_on("HTTParty.get(", Some("GET"), &["method_call"]),
        ApiPattern::client_on("HTTParty.post(", Some("POST"), &["method_call"]),
        ApiPattern::client_on("RestClient.get(", Some("GET"), &["method_call"]),
        ApiPattern::client_on("RestClient.post(", Some("POST"), &["method_call"]),
        ApiPattern::client_on("Faraday.get(", Some("GET"), &["method_call"]),
        ApiPattern::client_on("Faraday.post(", Some("POST"), &["method_call"]),
        ApiPattern::client_on("Typhoeus.get(", Some("GET"), &["method_call"]),
        ApiPattern::client_on("Typhoeus.post(", Some("POST"), &["method_call"]),
        // Server: Rails/Sinatra (standalone calls, no receiver)
        ApiPattern::server_on("get ", Some("GET"), &["call"]),
        ApiPattern::server_on("post ", Some("POST"), &["call"]),
        ApiPattern::server_on("put ", Some("PUT"), &["call"]),
        ApiPattern::server_on("delete ", Some("DELETE"), &["call"]),
        ApiPattern::server_on("patch ", Some("PATCH"), &["call"]),
        ApiPattern::server_on("match ", None, &["call"]),
    ],
};

const RUST: LangApiConfig = LangApiConfig {
    check_nodes: &["attribute_item", "call_expression"],
    fn_scope: &["function_item"],
    class_scope: &[],
    base_path_markers: &[],
    patterns: &[
        // Server: Rocket/Actix attribute macros
        ApiPattern::server_on("#[get(", Some("GET"), &["attribute_item"]),
        ApiPattern::server_on("#[post(", Some("POST"), &["attribute_item"]),
        ApiPattern::server_on("#[put(", Some("PUT"), &["attribute_item"]),
        ApiPattern::server_on("#[delete(", Some("DELETE"), &["attribute_item"]),
        ApiPattern::server_on("#[patch(", Some("PATCH"), &["attribute_item"]),
        ApiPattern::server_on("actix_web::get(", Some("GET"), &["attribute_item"]),
        ApiPattern::server_on("actix_web::post(", Some("POST"), &["attribute_item"]),
        // Server: Axum .route()
        ApiPattern::server_on(".route(", None, &["call_expression"]),
        // Client: reqwest
        ApiPattern::client_on("reqwest::get(", Some("GET"), &["call_expression"]),
        ApiPattern::client_on("reqwest::Client", None, &["call_expression"]),
        ApiPattern::client_on("client.get(", Some("GET"), &["call_expression"]),
        ApiPattern::client_on("client.post(", Some("POST"), &["call_expression"]),
        ApiPattern::client_on("client.put(", Some("PUT"), &["call_expression"]),
        ApiPattern::client_on("client.delete(", Some("DELETE"), &["call_expression"]),
    ],
};

const CPP: LangApiConfig = LangApiConfig {
    check_nodes: &["call_expression"],
    fn_scope: &["function_definition"],
    class_scope: &["class_specifier"],
    base_path_markers: &[],
    patterns: &[
        // Client: cpr library (check before server .Get patterns)
        ApiPattern::client("cpr::Get(", Some("GET")),
        ApiPattern::client("cpr::Post(", Some("POST")),
        ApiPattern::client("cpr::Put(", Some("PUT")),
        ApiPattern::client("cpr::Delete(", Some("DELETE")),
        ApiPattern::client("cpr::Patch(", Some("PATCH")),
        ApiPattern::client("cli.Get(", Some("GET")),
        ApiPattern::client("cli.Post(", Some("POST")),
        ApiPattern::client("client.Get(", Some("GET")),
        ApiPattern::client("client.Post(", Some("POST")),
        // Server: cpp-httplib
        ApiPattern::server("svr.Get(", Some("GET")),
        ApiPattern::server("svr.Post(", Some("POST")),
        ApiPattern::server("svr.Put(", Some("PUT")),
        ApiPattern::server("svr.Delete(", Some("DELETE")),
        ApiPattern::server("server.Get(", Some("GET")),
        ApiPattern::server("server.Post(", Some("POST")),
        // Server: Pistache
        ApiPattern::server("Routes::Get(", Some("GET")),
        ApiPattern::server("Routes::Post(", Some("POST")),
        ApiPattern::server("Routes::Put(", Some("PUT")),
        ApiPattern::server("Routes::Delete(", Some("DELETE")),
        // Server: Crow
        ApiPattern::server("CROW_ROUTE(", None),
    ],
};

const SWIFT: LangApiConfig = LangApiConfig {
    check_nodes: &["call_expression"],
    fn_scope: &["function_declaration"],
    class_scope: &["class_declaration", "struct_declaration"],
    base_path_markers: &[],
    patterns: &[
        // Client: URLSession, Alamofire (check before server .get patterns)
        ApiPattern::client("URLSession", None),
        ApiPattern::client("dataTask(", None),
        ApiPattern::client("URL(string:", None),
        ApiPattern::client("AF.request(", None),
        ApiPattern::client("Alamofire.request(", None),
        // Server: Vapor
        ApiPattern::server("app.get(", Some("GET")),
        ApiPattern::server("app.post(", Some("POST")),
        ApiPattern::server("app.put(", Some("PUT")),
        ApiPattern::server("app.delete(", Some("DELETE")),
        ApiPattern::server("app.patch(", Some("PATCH")),
        ApiPattern::server("router.get(", Some("GET")),
        ApiPattern::server("router.post(", Some("POST")),
    ],
};

// ── Public API ───────────────────────────────────────────────────────────────

/// Extract API endpoints from a parsed AST using pattern-driven detection.
pub fn extract_api_endpoints(
    root: &Node,
    source: &[u8],
    language: SupportedLanguage,
    file_path: &Path,
) -> Vec<ExtractedApiEndpoint> {
    let config = match language {
        SupportedLanguage::Python => &PYTHON,
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            &JAVASCRIPT
        }
        SupportedLanguage::Go => &GO,
        SupportedLanguage::Java => &JAVA,
        SupportedLanguage::CSharp => &CSHARP,
        SupportedLanguage::Ruby => &RUBY,
        SupportedLanguage::Rust => &RUST,
        SupportedLanguage::Cpp => &CPP,
        SupportedLanguage::Swift => &SWIFT,
    };

    let is_backend = is_backend_file(file_path);
    let mut endpoints = Vec::new();
    let mut base_path = String::new();
    walk_node(
        root,
        source,
        config,
        &mut endpoints,
        None,
        &mut base_path,
        is_backend,
    );
    endpoints
}

// ── Generic Walker ───────────────────────────────────────────────────────────

fn walk_node(
    node: &Node,
    source: &[u8],
    config: &LangApiConfig,
    endpoints: &mut Vec<ExtractedApiEndpoint>,
    current_scope: Option<&str>,
    base_path: &mut String,
    is_backend: bool,
) {
    let kind = node.kind();

    // ── Track scope ──────────────────────────────────────────────────────
    let new_scope = if config.fn_scope.contains(&kind) || config.class_scope.contains(&kind) {
        extract_scope_name(node, source)
    } else {
        None
    };
    let scope = new_scope.as_deref().or(current_scope);

    // ── Extract class-level base path (Java @RequestMapping, C# [Route]) ─
    if config.class_scope.contains(&kind) && !config.base_path_markers.is_empty() {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if let Ok(text) = child.utf8_text(source) {
                    if config.base_path_markers.iter().any(|m| text.contains(m)) {
                        if let Some(url) = extract_first_string(text) {
                            *base_path = url;
                            break;
                        }
                    }
                }
            }
        }
    }

    // ── Check node against patterns ──────────────────────────────────────
    if config.check_nodes.contains(&kind) {
        if let Ok(text) = node.utf8_text(source) {
            // Don't process huge nodes (class bodies, etc.)
            if text.len() < 2000 {
                for pattern in config.patterns {
                    // Node kind filter
                    if !pattern.only_on.is_empty() && !pattern.only_on.contains(&kind) {
                        continue;
                    }
                    // Backend-only filter
                    if pattern.backend_only && !is_backend {
                        continue;
                    }
                    // Text match
                    if !text.contains(pattern.text) {
                        continue;
                    }

                    // Extract URL
                    let raw_url = extract_first_string(text).unwrap_or_default();
                    let full_url = apply_base_path(&raw_url, base_path);

                    if full_url.is_empty() || !is_api_url(&full_url) {
                        break; // Pattern matched but no valid URL — skip remaining patterns too
                    }

                    // Resolve method
                    let method = pattern
                        .method
                        .map(|m| m.to_string())
                        .or_else(|| detect_method_from_text(text).map(|m| m.to_string()));

                    // Resolve scope: current scope, or peek at parent/siblings
                    let endpoint_scope = scope
                        .map(|s| s.to_string())
                        .or_else(|| resolve_scope(node, source, config.fn_scope));

                    let endpoint_kind = if pattern.is_server {
                        ApiEndpointKind::Defines
                    } else {
                        ApiEndpointKind::Consumes
                    };

                    endpoints.push(ExtractedApiEndpoint {
                        url: normalize_url(&full_url),
                        method,
                        kind: endpoint_kind,
                        scope: endpoint_scope,
                        line: node.start_position().row + 1,
                    });

                    break; // First match wins
                }
            }
        }
    }

    // ── Recurse ──────────────────────────────────────────────────────────
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            walk_node(
                &child, source, config, endpoints, scope, base_path, is_backend,
            );
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract a function/class name from a node.
fn extract_scope_name(node: &Node, source: &[u8]) -> Option<String> {
    // JS: variable_declarator only sets scope if value is a function
    if node.kind() == "variable_declarator" {
        let is_fn = node
            .child_by_field_name("value")
            .map(|v| {
                matches!(
                    v.kind(),
                    "arrow_function" | "function_expression" | "function"
                )
            })
            .unwrap_or(false);
        if !is_fn {
            return None;
        }
    }

    // Try "name" field (works for most languages)
    if let Some(name) = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
    {
        return Some(name.to_string());
    }

    // C++ fallback: declarator → declarator (nested)
    if let Some(decl) = node.child_by_field_name("declarator") {
        let inner = decl.child_by_field_name("declarator").unwrap_or(decl);
        if let Ok(text) = inner.utf8_text(source) {
            return Some(text.split('(').next().unwrap_or(text).to_string());
        }
    }

    None
}

/// Walk up parents and check siblings to find enclosing function scope.
fn resolve_scope(node: &Node, source: &[u8], fn_nodes: &[&str]) -> Option<String> {
    // Strategy 1: Walk up to find enclosing function (Java annotations, C# attributes, Rust attributes)
    let mut parent = node.parent();
    while let Some(p) = parent {
        if fn_nodes.contains(&p.kind()) {
            if let Some(name) = extract_scope_name(&p, source) {
                return Some(name);
            }
        }
        parent = p.parent();
    }

    // Strategy 2: Check siblings (Python: decorator → sibling function_definition)
    if let Some(p) = node.parent() {
        for i in 0..p.child_count() {
            if let Some(sibling) = p.child(i) {
                if fn_nodes.contains(&sibling.kind()) {
                    if let Some(name) = extract_scope_name(&sibling, source) {
                        return Some(name);
                    }
                }
            }
        }
    }

    None
}

/// Apply class-level base path to a URL.
fn apply_base_path(url: &str, base_path: &str) -> String {
    if base_path.is_empty() {
        return url.to_string();
    }
    if url.is_empty() {
        return base_path.to_string();
    }
    if url.starts_with(base_path) {
        return url.to_string();
    }
    let base = base_path.trim_end_matches('/');
    let suffix = if url.starts_with('/') {
        url.to_string()
    } else {
        format!("/{}", url)
    };
    format!("{}{}", base, suffix)
}

/// Extract the first quoted string from text.
fn extract_first_string(text: &str) -> Option<String> {
    // Double quotes
    if let Some(start) = text.find('"') {
        if let Some(end) = text[start + 1..].find('"') {
            return Some(text[start + 1..start + 1 + end].to_string());
        }
    }
    // Single quotes (only if it looks like a URL path)
    if let Some(start) = text.find('\'') {
        if let Some(end) = text[start + 1..].find('\'') {
            let s = &text[start + 1..start + 1 + end];
            if s.starts_with('/') || s.contains("api") || s.starts_with("http") {
                return Some(s.to_string());
            }
        }
    }
    // Backticks (JS template literals)
    if let Some(start) = text.find('`') {
        if let Some(end) = text[start + 1..].find('`') {
            return Some(text[start + 1..start + 1 + end].to_string());
        }
    }
    None
}

/// Auto-detect HTTP method from node text when pattern doesn't specify one.
fn detect_method_from_text(text: &str) -> Option<&'static str> {
    // Check for specific method indicators in the text
    if text.contains(".get(")
        || text.contains(".Get(")
        || text.contains(".GET(")
        || text.contains("\"GET\"")
    {
        return Some("GET");
    }
    if text.contains(".post(")
        || text.contains(".Post(")
        || text.contains(".POST(")
        || text.contains("\"POST\"")
    {
        return Some("POST");
    }
    if text.contains(".put(")
        || text.contains(".Put(")
        || text.contains(".PUT(")
        || text.contains("\"PUT\"")
    {
        return Some("PUT");
    }
    if text.contains(".delete(")
        || text.contains(".Delete(")
        || text.contains(".DELETE(")
        || text.contains("\"DELETE\"")
    {
        return Some("DELETE");
    }
    if text.contains(".patch(")
        || text.contains(".Patch(")
        || text.contains(".PATCH(")
        || text.contains("\"PATCH\"")
    {
        return Some("PATCH");
    }
    Some("GET") // Default
}

/// Normalize URL by replacing all param styles with :param.
fn normalize_url(url: &str) -> String {
    let mut result = String::new();
    let mut chars = url.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // {id} — Python, Java, C#, Rust
            '{' => {
                for c2 in chars.by_ref() {
                    if c2 == '}' {
                        break;
                    }
                }
                result.push_str(":param");
            }
            // <id> or <int:id> — Flask/Werkzeug
            '<' => {
                for c2 in chars.by_ref() {
                    if c2 == '>' {
                        break;
                    }
                }
                result.push_str(":param");
            }
            // ${id} — JS template literal
            '$' if chars.peek() == Some(&'{') => {
                chars.next();
                let mut depth = 1;
                for c2 in chars.by_ref() {
                    if c2 == '{' {
                        depth += 1;
                    } else if c2 == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                result.push_str(":param");
            }
            // :id — Express, Ruby, Go
            ':' if chars.peek().is_some_and(|c| c.is_alphabetic()) => {
                while chars
                    .peek()
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_')
                {
                    chars.next();
                }
                result.push_str(":param");
            }
            // *filepath — Go catch-all
            '*' if chars.peek().is_some_and(|c| c.is_alphabetic()) => {
                while chars
                    .peek()
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_')
                {
                    chars.next();
                }
                result.push_str(":param");
            }
            _ => result.push(c),
        }
    }
    result
}

/// Check if URL looks like an API endpoint.
fn is_api_url(url: &str) -> bool {
    let url = url.to_lowercase();
    url.starts_with("/api/")
        || url.starts_with("/v1/")
        || url.starts_with("/v2/")
        || url.starts_with("/v3/")
        || url.contains("/api/")
        || url.starts_with("http://")
        || url.starts_with("https://")
        || url.contains("[controller]")
        || (url.starts_with('/') && url.len() > 1 && !url.contains('.'))
}

/// Heuristic: is this JS/TS file likely backend code?
fn is_backend_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    path_str.contains("/server/")
        || path_str.contains("/backend/")
        || path_str.contains("/api/")
        || path_str.contains("/routes/")
        || path_str.contains("/controllers/")
        || path_str.contains("/handlers/")
        || path_str.contains("server.")
        || path_str.contains("app.")
        || path_str.ends_with(".server.ts")
        || path_str.ends_with(".server.js")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_normalize_url() {
        assert_eq!(normalize_url("/api/users/{id}"), "/api/users/:param");
        assert_eq!(normalize_url("/api/users/<int:id>"), "/api/users/:param");
        assert_eq!(normalize_url("/api/users/:userId"), "/api/users/:param");
        assert_eq!(normalize_url("/api/users/${id}"), "/api/users/:param");
        assert_eq!(normalize_url("/api/files/*filepath"), "/api/files/:param");
        assert_eq!(
            normalize_url("/api/items/{item_id}/comments/{cid}"),
            "/api/items/:param/comments/:param"
        );
    }

    #[test]
    fn test_is_api_url() {
        assert!(is_api_url("/api/users"));
        assert!(is_api_url("/v1/products"));
        assert!(is_api_url("/users"));
        assert!(is_api_url("https://api.example.com/users"));
        assert!(!is_api_url(""));
        assert!(!is_api_url("/static/styles.css"));
    }

    #[test]
    fn test_extract_first_string() {
        assert_eq!(
            extract_first_string(r#"app.get("/api/users")"#),
            Some("/api/users".to_string())
        );
        assert_eq!(
            extract_first_string("get '/api/items'"),
            Some("/api/items".to_string())
        );
        assert_eq!(
            extract_first_string("fetch(`/api/data`)"),
            Some("/api/data".to_string())
        );
        assert_eq!(extract_first_string("no_strings_here"), None);
    }

    #[test]
    fn test_apply_base_path() {
        assert_eq!(apply_base_path("/users", "/api/v1"), "/api/v1/users");
        assert_eq!(apply_base_path("", "/api/inventory"), "/api/inventory");
        assert_eq!(apply_base_path("/users", ""), "/users");
        assert_eq!(apply_base_path("/api/v1/users", "/api/v1"), "/api/v1/users");
    }

    #[test]
    fn test_is_backend_file() {
        assert!(is_backend_file(&PathBuf::from("/project/server/index.ts")));
        assert!(is_backend_file(&PathBuf::from(
            "/project/api/routes/users.js"
        )));
        assert!(is_backend_file(&PathBuf::from("/project/app.server.ts")));
        assert!(!is_backend_file(&PathBuf::from(
            "/project/src/components/Button.tsx"
        )));
        assert!(!is_backend_file(&PathBuf::from("/project/pages/index.tsx")));
    }

    #[test]
    fn test_detect_method_from_text() {
        assert_eq!(
            detect_method_from_text(".route(\"/api\", get(handler))"),
            Some("GET")
        );
        assert_eq!(detect_method_from_text("method: \"POST\""), Some("POST"));
        assert_eq!(
            detect_method_from_text(".Delete(\"/api/users\")"),
            Some("DELETE")
        );
    }
}

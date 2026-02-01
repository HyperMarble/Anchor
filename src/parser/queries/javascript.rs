//! JavaScript/TypeScript API endpoint detection via AST traversal.
//!
//! Detects:
//! - Frontend API calls: fetch(), axios.get(), etc.
//! - Backend route definitions: app.get(), router.post(), etc.

use tree_sitter::Node;
use crate::graph::types::{ExtractedApiEndpoint, ApiEndpointKind};

/// Extract API endpoints from JavaScript/TypeScript AST.
pub fn extract_js_apis(
    root: &Node,
    source: &[u8],
    is_likely_backend: bool,
) -> Vec<ExtractedApiEndpoint> {
    let mut endpoints = Vec::new();
    extract_from_node(root, source, is_likely_backend, &mut endpoints, None);
    endpoints
}

/// Recursively walk AST and extract API endpoints.
fn extract_from_node(
    node: &Node,
    source: &[u8],
    is_likely_backend: bool,
    endpoints: &mut Vec<ExtractedApiEndpoint>,
    current_scope: Option<&str>,
) {
    let kind = node.kind();

    // Track scope for function names
    let new_scope = match kind {
        "function_declaration" | "method_definition" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        }
        "variable_declarator" => {
            // const fetchUsers = async () => {}
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        }
        _ => None,
    };
    let scope = new_scope.as_deref().or(current_scope);

    // Check for API-related call expressions
    if kind == "call_expression" {
        if let Some(endpoint) = extract_api_from_call(node, source, is_likely_backend, scope) {
            endpoints.push(endpoint);
        }
    }

    // Recurse into children
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            extract_from_node(&child, source, is_likely_backend, endpoints, scope);
        }
    }
}

/// Check if a call_expression is an API call and extract endpoint info.
fn extract_api_from_call(
    node: &Node,
    source: &[u8],
    is_likely_backend: bool,
    scope: Option<&str>,
) -> Option<ExtractedApiEndpoint> {
    let func_node = node.child_by_field_name("function")?;
    let args_node = node.child_by_field_name("arguments")?;

    // Get the first argument (usually the URL)
    let first_arg = get_first_string_arg(&args_node, source)?;

    // Check if it looks like an API URL
    if !is_api_url(&first_arg) {
        return None;
    }

    let func_kind = func_node.kind();

    match func_kind {
        // Direct function call: fetch("/api/users")
        "identifier" => {
            let func_name = func_node.utf8_text(source).ok()?;

            if func_name == "fetch" || func_name == "request" {
                return Some(ExtractedApiEndpoint {
                    url: normalize_url(&first_arg),
                    method: detect_fetch_method(node, source),
                    kind: ApiEndpointKind::Consumes,
                    scope: scope.map(|s| s.to_string()),
                    line: node.start_position().row + 1,
                });
            }
        }
        // Member expression: axios.get("/api/users") or app.route("/api/users")
        "member_expression" => {
            let obj = func_node.child_by_field_name("object")?;
            let prop = func_node.child_by_field_name("property")?;

            let obj_name = obj.utf8_text(source).ok()?;
            let method_name = prop.utf8_text(source).ok()?;

            // Frontend: axios, http, api, $, ky, got
            let frontend_objects = ["axios", "http", "api", "$", "ky", "got", "client", "request"];

            // Backend: app, router, server, express, fastify, hono
            let backend_objects = ["app", "router", "server", "express", "fastify", "hono", "koa"];

            // HTTP methods
            let http_methods = ["get", "post", "put", "delete", "patch", "head", "options"];
            let route_methods = ["get", "post", "put", "delete", "patch", "all", "use", "route"];

            if frontend_objects.contains(&obj_name) && http_methods.contains(&method_name) {
                return Some(ExtractedApiEndpoint {
                    url: normalize_url(&first_arg),
                    method: Some(method_name.to_uppercase()),
                    kind: ApiEndpointKind::Consumes,
                    scope: scope.map(|s| s.to_string()),
                    line: node.start_position().row + 1,
                });
            }

            if is_likely_backend && backend_objects.contains(&obj_name) && route_methods.contains(&method_name) {
                let http_method = if method_name == "route" || method_name == "use" || method_name == "all" {
                    None
                } else {
                    Some(method_name.to_uppercase())
                };

                return Some(ExtractedApiEndpoint {
                    url: normalize_url(&first_arg),
                    method: http_method,
                    kind: ApiEndpointKind::Defines,
                    scope: scope.map(|s| s.to_string()),
                    line: node.start_position().row + 1,
                });
            }
        }
        _ => {}
    }

    None
}

/// Get the first string argument from an arguments node.
fn get_first_string_arg(args_node: &Node, source: &[u8]) -> Option<String> {
    let count = args_node.child_count();

    for i in 0..count {
        if let Some(child) = args_node.child(i) {
            let kind = child.kind();

            match kind {
                "string" => {
                    // "url" or 'url'
                    let text = child.utf8_text(source).ok()?;
                    return Some(strip_quotes(text));
                }
                "template_string" => {
                    // `url` or `/api/users/${id}`
                    let text = child.utf8_text(source).ok()?;
                    return Some(strip_quotes(text));
                }
                _ => continue,
            }
        }
    }
    None
}

/// Detect HTTP method from fetch() options: fetch(url, { method: "POST" })
fn detect_fetch_method(call_node: &Node, source: &[u8]) -> Option<String> {
    let args = call_node.child_by_field_name("arguments")?;
    let count = args.child_count();

    // Look for options object argument
    for i in 0..count {
        if let Some(child) = args.child(i) {
            if child.kind() == "object" {
                return extract_method_from_object(&child, source);
            }
        }
    }

    // Default to GET
    Some("GET".to_string())
}

/// Extract method value from { method: "POST" } object.
fn extract_method_from_object(obj_node: &Node, source: &[u8]) -> Option<String> {
    let count = obj_node.child_count();

    for i in 0..count {
        if let Some(child) = obj_node.child(i) {
            if child.kind() == "pair" {
                let key = child.child_by_field_name("key")?;
                let key_text = key.utf8_text(source).ok()?;

                if key_text == "method" || key_text == "\"method\"" || key_text == "'method'" {
                    let value = child.child_by_field_name("value")?;
                    let value_text = value.utf8_text(source).ok()?;
                    let method = strip_quotes(value_text).to_uppercase();
                    return Some(method);
                }
            }
        }
    }

    Some("GET".to_string())
}

/// Strip quotes from a string.
fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() < 2 {
        return s.to_string();
    }

    let first = s.chars().next().unwrap();
    let last = s.chars().last().unwrap();

    if (first == '"' && last == '"')
        || (first == '\'' && last == '\'')
        || (first == '`' && last == '`')
    {
        s[1..s.len()-1].to_string()
    } else {
        s.to_string()
    }
}

/// Normalize URL by converting template variables to :param.
fn normalize_url(url: &str) -> String {
    let mut result = String::new();
    let mut chars = url.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // Template literal: ${id}
            '$' if chars.peek() == Some(&'{') => {
                chars.next(); // consume '{'
                let mut depth = 1;
                while let Some(c2) = chars.next() {
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
            // Express/path style: :id (but not ::)
            ':' if chars.peek().map_or(false, |c| c.is_alphabetic()) => {
                result.push(':');
                while chars.peek().map_or(false, |c| c.is_alphanumeric() || *c == '_') {
                    chars.next();
                }
                result.push_str("param");
            }
            // Curly brace style: {id}
            '{' => {
                while let Some(c2) = chars.next() {
                    if c2 == '}' {
                        break;
                    }
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
        || (url.starts_with('/') && url.len() > 1 && !url.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url() {
        assert_eq!(normalize_url("/api/users/${id}"), "/api/users/:param");
        assert_eq!(normalize_url("/api/users/:userId"), "/api/users/:param");
        assert_eq!(normalize_url("/api/items/{item_id}/comments"), "/api/items/:param/comments");
        assert_eq!(normalize_url("/api/users/${user.id}"), "/api/users/:param");
    }

    #[test]
    fn test_is_api_url() {
        assert!(is_api_url("/api/users"));
        assert!(is_api_url("https://api.example.com/users"));
        assert!(is_api_url("/v1/products"));
        assert!(is_api_url("/users"));
        assert!(!is_api_url(""));
        assert!(!is_api_url("/styles.css"));
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes("\"hello\""), "hello");
        assert_eq!(strip_quotes("'hello'"), "hello");
        assert_eq!(strip_quotes("`hello`"), "hello");
        assert_eq!(strip_quotes("hello"), "hello");
    }
}

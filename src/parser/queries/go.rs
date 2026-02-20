//
//  go.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;
use crate::graph::types::{ExtractedApiEndpoint, ApiEndpointKind};

/// Extract API endpoints from Go AST.
pub fn extract_go_apis(root: &Node, source: &[u8]) -> Vec<ExtractedApiEndpoint> {
    let mut endpoints = Vec::new();
    extract_from_node(root, source, &mut endpoints, None);
    endpoints
}

fn extract_from_node(
    node: &Node,
    source: &[u8],
    endpoints: &mut Vec<ExtractedApiEndpoint>,
    current_scope: Option<&str>,
) {
    let kind = node.kind();

    // Track function scope
    let new_scope = if kind == "function_declaration" {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string())
    } else {
        None
    };
    let scope = new_scope.as_deref().or(current_scope);

    // Check for route definitions
    if kind == "call_expression" {
        if let Some(endpoint) = extract_route_from_call(node, source, scope) {
            endpoints.push(endpoint);
        }
    }

    // Recurse
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            extract_from_node(&child, source, endpoints, scope);
        }
    }
}

fn extract_route_from_call(
    node: &Node,
    source: &[u8],
    scope: Option<&str>,
) -> Option<ExtractedApiEndpoint> {
    let func = node.child_by_field_name("function")?;
    let args = node.child_by_field_name("arguments")?;

    // We're looking for patterns like:
    // r.GET("/api/users", handler)
    // e.POST("/api/users", handler)
    // http.HandleFunc("/api/users", handler)

    if func.kind() != "selector_expression" {
        return None;
    }

    let _obj = func.child_by_field_name("operand")?;
    let method = func.child_by_field_name("field")?;

    let method_name = method.utf8_text(source).ok()?;

    // Check for HTTP methods (Gin, Echo, Chi, Fiber style)
    let http_method = match method_name.to_uppercase().as_str() {
        "GET" | "Get" => Some("GET"),
        "POST" | "Post" => Some("POST"),
        "PUT" | "Put" => Some("PUT"),
        "DELETE" | "Delete" => Some("DELETE"),
        "PATCH" | "Patch" => Some("PATCH"),
        "HEAD" | "Head" => Some("HEAD"),
        "OPTIONS" | "Options" => Some("OPTIONS"),
        "ANY" | "Any" => None, // Any method
        "HANDLE" | "Handle" | "HandleFunc" | "HANDLEFUNC" => Some("GET"), // Default for http.HandleFunc
        "GROUP" | "Group" => return None, // Route groups, not endpoints
        _ => return None,
    };

    // Get URL from first argument
    let url = get_first_string_arg(&args, source)?;

    if !is_api_url(&url) {
        return None;
    }

    Some(ExtractedApiEndpoint {
        url: normalize_url(&url),
        method: http_method.map(|s| s.to_string()),
        kind: ApiEndpointKind::Defines,
        scope: scope.map(|s| s.to_string()),
        line: node.start_position().row + 1,
    })
}

fn get_first_string_arg(args: &Node, source: &[u8]) -> Option<String> {
    let count = args.child_count();
    for i in 0..count {
        if let Some(child) = args.child(i) {
            if child.kind() == "interpreted_string_literal" || child.kind() == "raw_string_literal" {
                let text = child.utf8_text(source).ok()?;
                return Some(strip_quotes(text));
            }
        }
    }
    None
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() < 2 {
        return s.to_string();
    }

    // Handle `raw string` or "string"
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('`') && s.ends_with('`')) {
        s[1..s.len()-1].to_string()
    } else {
        s.to_string()
    }
}

fn normalize_url(url: &str) -> String {
    let mut result = String::new();
    let mut chars = url.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            // Go path params: :id or *filepath
            ':' | '*' => {
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

fn is_api_url(url: &str) -> bool {
    let url = url.to_lowercase();
    url.starts_with("/api/")
        || url.starts_with("/v1/")
        || url.starts_with("/v2/")
        || url.starts_with("/v3/")
        || url.contains("/api/")
        || (url.starts_with('/') && url.len() > 1 && !url.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url() {
        assert_eq!(normalize_url("/api/users/:id"), "/api/users/:param");
        assert_eq!(normalize_url("/api/files/*filepath"), "/api/files/:param");
    }
}

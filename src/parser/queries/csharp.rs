//
//  csharp.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;
use crate::graph::types::{ExtractedApiEndpoint, ApiEndpointKind};

/// Extract API endpoints from C# AST.
pub fn extract_csharp_apis(root: &Node, source: &[u8]) -> Vec<ExtractedApiEndpoint> {
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

    // Track method scope
    let new_scope = if kind == "method_declaration" {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string())
    } else {
        None
    };
    let scope = new_scope.as_deref().or(current_scope);

    // Check for attribute-decorated methods
    if kind == "method_declaration" {
        if let Some(endpoint) = extract_route_from_method(node, source, scope) {
            endpoints.push(endpoint);
        }
    }

    // Check for minimal API patterns: app.MapGet("/api/users", ...)
    if kind == "invocation_expression" {
        if let Some(endpoint) = extract_minimal_api_route(node, source, scope) {
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

fn extract_route_from_method(
    node: &Node,
    source: &[u8],
    scope: Option<&str>,
) -> Option<ExtractedApiEndpoint> {
    // Look for attributes on method
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            if child.kind() == "attribute_list" {
                let attr_count = child.child_count();
                for j in 0..attr_count {
                    if let Some(attr) = child.child(j) {
                        if attr.kind() == "attribute" {
                            if let Some(endpoint) = extract_endpoint_from_attribute(&attr, source, scope, node.start_position().row + 1) {
                                return Some(endpoint);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_endpoint_from_attribute(
    node: &Node,
    source: &[u8],
    scope: Option<&str>,
    line: usize,
) -> Option<ExtractedApiEndpoint> {
    let name = node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())?;

    let http_method = match name {
        "HttpGet" => Some("GET"),
        "HttpPost" => Some("POST"),
        "HttpPut" => Some("PUT"),
        "HttpDelete" => Some("DELETE"),
        "HttpPatch" => Some("PATCH"),
        "Route" => None,
        _ => return None,
    };

    let url = extract_path_from_attribute(node, source)?;

    if !is_api_url(&url) {
        return None;
    }

    Some(ExtractedApiEndpoint {
        url: normalize_url(&url),
        method: http_method.map(|s| s.to_string()),
        kind: ApiEndpointKind::Defines,
        scope: scope.map(|s| s.to_string()),
        line,
    })
}

fn extract_minimal_api_route(
    node: &Node,
    source: &[u8],
    scope: Option<&str>,
) -> Option<ExtractedApiEndpoint> {
    // app.MapGet("/api/users", handler)
    let text = node.utf8_text(source).ok()?;

    let http_method = if text.contains("MapGet") {
        Some("GET")
    } else if text.contains("MapPost") {
        Some("POST")
    } else if text.contains("MapPut") {
        Some("PUT")
    } else if text.contains("MapDelete") {
        Some("DELETE")
    } else if text.contains("MapPatch") {
        Some("PATCH")
    } else {
        return None;
    };

    // Extract URL from arguments
    let args = node.child_by_field_name("arguments")?;
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

fn extract_path_from_attribute(node: &Node, source: &[u8]) -> Option<String> {
    let args = node.child_by_field_name("arguments")?;

    let count = args.child_count();
    for i in 0..count {
        if let Some(child) = args.child(i) {
            if child.kind() == "attribute_argument" || child.kind() == "string_literal" {
                let text = child.utf8_text(source).ok()?;
                return Some(strip_quotes(text));
            }
        }
    }
    None
}

fn get_first_string_arg(args: &Node, source: &[u8]) -> Option<String> {
    let count = args.child_count();
    for i in 0..count {
        if let Some(child) = args.child(i) {
            if child.kind() == "argument" {
                let text = child.utf8_text(source).ok()?;
                if text.contains('"') {
                    return Some(strip_quotes(text));
                }
            }
        }
    }
    None
}

fn strip_quotes(s: &str) -> String {
    // Extract string content from "..." or @"..."
    let s = s.trim();
    if let Some(start) = s.find('"') {
        if let Some(end) = s.rfind('"') {
            if end > start {
                return s[start+1..end].to_string();
            }
        }
    }
    s.to_string()
}

fn normalize_url(url: &str) -> String {
    let mut result = String::new();
    let mut chars = url.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            // ASP.NET route parameter: {id}
            while let Some(c2) = chars.next() {
                if c2 == '}' {
                    break;
                }
            }
            result.push_str(":param");
        } else {
            result.push(c);
        }
    }

    result
}

fn is_api_url(url: &str) -> bool {
    let url = url.to_lowercase();
    url.starts_with("/api/")
        || url.starts_with("/v1/")
        || url.starts_with("/v2/")
        || url.contains("/api/")
        || url.contains("[controller]")
        || (url.starts_with('/') && url.len() > 1 && !url.contains('.'))
}

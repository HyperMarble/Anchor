//
//  kotlin.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;
use crate::graph::types::{ExtractedApiEndpoint, ApiEndpointKind};

/// Extract API endpoints from Kotlin AST.
pub fn extract_kotlin_apis(root: &Node, source: &[u8]) -> Vec<ExtractedApiEndpoint> {
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
        node.child_by_field_name("simple_identifier")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string())
    } else {
        None
    };
    let scope = new_scope.as_deref().or(current_scope);

    // Check for Ktor routes: get("/api/users") { ... }
    if kind == "call_expression" {
        if let Some(endpoint) = extract_ktor_route(node, source, scope) {
            endpoints.push(endpoint);
        }
    }

    // Check for Spring annotations
    if kind == "function_declaration" {
        if let Some(endpoint) = extract_spring_route(node, source, scope) {
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

fn extract_ktor_route(
    node: &Node,
    source: &[u8],
    scope: Option<&str>,
) -> Option<ExtractedApiEndpoint> {
    // Get the function name
    let func_name = node.child(0)
        .and_then(|n| n.utf8_text(source).ok())?;

    let http_method = match func_name {
        "get" => Some("GET"),
        "post" => Some("POST"),
        "put" => Some("PUT"),
        "delete" => Some("DELETE"),
        "patch" => Some("PATCH"),
        "head" => Some("HEAD"),
        "options" => Some("OPTIONS"),
        "route" => None,
        _ => return None,
    };

    // Get URL from arguments
    let url = extract_first_string_from_node(node, source)?;

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

fn extract_spring_route(
    node: &Node,
    source: &[u8],
    scope: Option<&str>,
) -> Option<ExtractedApiEndpoint> {
    // Look for annotations
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            if child.kind() == "modifiers" {
                let mod_count = child.child_count();
                for j in 0..mod_count {
                    if let Some(annotation) = child.child(j) {
                        if annotation.kind() == "annotation" {
                            if let Some(endpoint) = extract_endpoint_from_annotation(&annotation, source, scope, node.start_position().row + 1) {
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

fn extract_endpoint_from_annotation(
    node: &Node,
    source: &[u8],
    scope: Option<&str>,
    line: usize,
) -> Option<ExtractedApiEndpoint> {
    let text = node.utf8_text(source).ok()?;

    let http_method = if text.contains("GetMapping") {
        Some("GET")
    } else if text.contains("PostMapping") {
        Some("POST")
    } else if text.contains("PutMapping") {
        Some("PUT")
    } else if text.contains("DeleteMapping") {
        Some("DELETE")
    } else if text.contains("PatchMapping") {
        Some("PATCH")
    } else if text.contains("RequestMapping") {
        None
    } else {
        return None;
    };

    let url = extract_first_string_from_node(node, source)?;

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

fn extract_first_string_from_node(node: &Node, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?;

    // Find first quoted string
    if let Some(start) = text.find('"') {
        if let Some(end) = text[start+1..].find('"') {
            return Some(text[start+1..start+1+end].to_string());
        }
    }
    None
}

fn normalize_url(url: &str) -> String {
    let mut result = String::new();
    let mut chars = url.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            // Ktor/Spring path parameter: {id}
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
        || (url.starts_with('/') && url.len() > 1 && !url.contains('.'))
}

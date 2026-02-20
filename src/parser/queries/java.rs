//
//  java.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;
use crate::graph::types::{ExtractedApiEndpoint, ApiEndpointKind};

/// Extract API endpoints from Java AST.
pub fn extract_java_apis(root: &Node, source: &[u8]) -> Vec<ExtractedApiEndpoint> {
    let mut endpoints = Vec::new();
    let mut base_path = String::new();
    extract_from_node(root, source, &mut endpoints, None, &mut base_path);
    endpoints
}

fn extract_from_node(
    node: &Node,
    source: &[u8],
    endpoints: &mut Vec<ExtractedApiEndpoint>,
    current_scope: Option<&str>,
    base_path: &mut String,
) {
    let kind = node.kind();

    // Track class-level @RequestMapping for base path
    if kind == "class_declaration" {
        if let Some(path) = extract_class_base_path(node, source) {
            *base_path = path;
        }
    }

    // Track method scope
    let new_scope = if kind == "method_declaration" {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string())
    } else {
        None
    };
    let scope = new_scope.as_deref().or(current_scope);

    // Check for route annotations on methods
    if kind == "method_declaration" {
        if let Some(mut endpoint) = extract_route_from_method(node, source, scope) {
            // Prepend base path
            if !base_path.is_empty() && !endpoint.url.starts_with(&*base_path) {
                endpoint.url = format!("{}{}", base_path.trim_end_matches('/'), endpoint.url);
            }
            endpoints.push(endpoint);
        }
    }

    // Recurse
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            extract_from_node(&child, source, endpoints, scope, base_path);
        }
    }
}

fn extract_class_base_path(node: &Node, source: &[u8]) -> Option<String> {
    // Look for @RequestMapping or @RestController on class
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            if child.kind() == "modifiers" {
                let mod_count = child.child_count();
                for j in 0..mod_count {
                    if let Some(annotation) = child.child(j) {
                        if annotation.kind() == "annotation" || annotation.kind() == "marker_annotation" {
                            if let Some(path) = extract_path_from_annotation(&annotation, source) {
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_route_from_method(
    node: &Node,
    source: &[u8],
    scope: Option<&str>,
) -> Option<ExtractedApiEndpoint> {
    // Look for annotations on method
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            if child.kind() == "modifiers" {
                let mod_count = child.child_count();
                for j in 0..mod_count {
                    if let Some(annotation) = child.child(j) {
                        if annotation.kind() == "annotation" || annotation.kind() == "marker_annotation" {
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
    let name = node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())?;

    let http_method = match name {
        "GetMapping" => Some("GET"),
        "PostMapping" => Some("POST"),
        "PutMapping" => Some("PUT"),
        "DeleteMapping" => Some("DELETE"),
        "PatchMapping" => Some("PATCH"),
        "RequestMapping" => None, // Need to check method attribute
        _ => return None,
    };

    let url = extract_path_from_annotation(node, source)?;

    if !is_api_url(&url) {
        return None;
    }

    // For @RequestMapping, try to extract method
    let final_method = if http_method.is_none() {
        extract_request_method(node, source).or(Some("GET".to_string()))
    } else {
        http_method.map(|s| s.to_string())
    };

    Some(ExtractedApiEndpoint {
        url: normalize_url(&url),
        method: final_method,
        kind: ApiEndpointKind::Defines,
        scope: scope.map(|s| s.to_string()),
        line,
    })
}

fn extract_path_from_annotation(node: &Node, source: &[u8]) -> Option<String> {
    let args = node.child_by_field_name("arguments")?;

    let count = args.child_count();
    for i in 0..count {
        if let Some(child) = args.child(i) {
            match child.kind() {
                "string_literal" => {
                    let text = child.utf8_text(source).ok()?;
                    return Some(strip_quotes(text));
                }
                "element_value_pair" => {
                    // value = "/api/users" or path = "/api/users"
                    let name = child.child_by_field_name("key")
                        .and_then(|n| n.utf8_text(source).ok())?;
                    if name == "value" || name == "path" {
                        let value = child.child_by_field_name("value")?;
                        let text = value.utf8_text(source).ok()?;
                        return Some(strip_quotes(text));
                    }
                }
                _ => continue,
            }
        }
    }
    None
}

fn extract_request_method(node: &Node, source: &[u8]) -> Option<String> {
    let args = node.child_by_field_name("arguments")?;

    let count = args.child_count();
    for i in 0..count {
        if let Some(child) = args.child(i) {
            if child.kind() == "element_value_pair" {
                let name = child.child_by_field_name("key")
                    .and_then(|n| n.utf8_text(source).ok())?;
                if name == "method" {
                    let value = child.child_by_field_name("value")
                        .and_then(|n| n.utf8_text(source).ok())?;

                    if value.contains("GET") { return Some("GET".to_string()); }
                    if value.contains("POST") { return Some("POST".to_string()); }
                    if value.contains("PUT") { return Some("PUT".to_string()); }
                    if value.contains("DELETE") { return Some("DELETE".to_string()); }
                    if value.contains("PATCH") { return Some("PATCH".to_string()); }
                }
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
    if s.starts_with('"') && s.ends_with('"') {
        s[1..s.len()-1].to_string()
    } else {
        s.to_string()
    }
}

fn normalize_url(url: &str) -> String {
    let mut result = String::new();
    let mut chars = url.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            // Spring path variable: {id}
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

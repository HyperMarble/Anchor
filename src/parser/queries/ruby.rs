//
//  ruby.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;
use crate::graph::types::{ExtractedApiEndpoint, ApiEndpointKind};

/// Extract API endpoints from Ruby AST.
pub fn extract_ruby_apis(root: &Node, source: &[u8]) -> Vec<ExtractedApiEndpoint> {
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

    // Track method/block scope
    let new_scope = if kind == "method" || kind == "singleton_method" {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string())
    } else {
        None
    };
    let scope = new_scope.as_deref().or(current_scope);

    // Check for route definitions
    if kind == "call" || kind == "method_call" {
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
    let method = node.child_by_field_name("method")
        .and_then(|n| n.utf8_text(source).ok())?;

    // Rails/Sinatra HTTP methods
    let http_method = match method {
        "get" => Some("GET"),
        "post" => Some("POST"),
        "put" => Some("PUT"),
        "patch" => Some("PATCH"),
        "delete" => Some("DELETE"),
        "match" => None, // Could be any method
        _ => return None,
    };

    // Get arguments
    let args = node.child_by_field_name("arguments")?;

    // First argument should be the path
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
            match child.kind() {
                "string" | "simple_string" | "string_content" => {
                    let text = child.utf8_text(source).ok()?;
                    return Some(strip_quotes(text));
                }
                _ => continue,
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

    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        s[1..s.len()-1].to_string()
    } else {
        s.to_string()
    }
}

fn normalize_url(url: &str) -> String {
    let mut result = String::new();
    let mut chars = url.chars().peekable();

    while let Some(c) = chars.next() {
        if c == ':' {
            // Rails route parameter: :id
            result.push(':');
            while chars.peek().map_or(false, |c| c.is_alphanumeric() || *c == '_') {
                chars.next();
            }
            result.push_str("param");
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

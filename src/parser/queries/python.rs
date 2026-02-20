//
//  python.rs
//  Anchor
//
//  Created by hak (tharun)
//

use tree_sitter::Node;
use crate::graph::types::{ExtractedApiEndpoint, ApiEndpointKind};

/// Extract API endpoints from Python AST.
pub fn extract_python_apis(root: &Node, source: &[u8]) -> Vec<ExtractedApiEndpoint> {
    let mut endpoints = Vec::new();
    extract_from_node(root, source, &mut endpoints);
    endpoints
}

/// Recursively walk AST and extract API endpoints from decorated functions.
fn extract_from_node(
    node: &Node,
    source: &[u8],
    endpoints: &mut Vec<ExtractedApiEndpoint>,
) {
    let kind = node.kind();

    // Look for decorated_definition (function with decorators)
    if kind == "decorated_definition" {
        if let Some(endpoint) = extract_api_from_decorated(node, source) {
            endpoints.push(endpoint);
        }
    }

    // Recurse into children
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            extract_from_node(&child, source, endpoints);
        }
    }
}

/// Extract API endpoint from a decorated function definition.
fn extract_api_from_decorated(node: &Node, source: &[u8]) -> Option<ExtractedApiEndpoint> {
    // Find the decorator(s)
    let mut url: Option<String> = None;
    let mut http_method: Option<String> = None;
    let mut func_name: Option<String> = None;
    let line = node.start_position().row + 1;

    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "decorator" => {
                    if let Some((u, m)) = extract_route_from_decorator(&child, source) {
                        url = Some(u);
                        http_method = m;
                    }
                }
                "function_definition" => {
                    func_name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.to_string());
                }
                _ => {}
            }
        }
    }

    let url = url?;
    if !is_api_url(&url) {
        return None;
    }

    Some(ExtractedApiEndpoint {
        url: normalize_url(&url),
        method: http_method,
        kind: ApiEndpointKind::Defines,
        scope: func_name,
        line,
    })
}

/// Extract route URL and HTTP method from a decorator.
fn extract_route_from_decorator(node: &Node, source: &[u8]) -> Option<(String, Option<String>)> {
    // Decorator structure: @ followed by expression
    // We're looking for:
    //   @app.route("/api/users")
    //   @app.get("/api/users")
    //   @router.post("/api/items")
    //   @bp.delete("/api/items/{id}")

    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            if child.kind() == "call" {
                return extract_route_from_call(&child, source);
            }
        }
    }
    None
}

/// Extract route info from a decorator call like app.route("/api/users").
fn extract_route_from_call(node: &Node, source: &[u8]) -> Option<(String, Option<String>)> {
    let func = node.child_by_field_name("function")?;
    let args = node.child_by_field_name("arguments")?;

    // Check if this is a route decorator
    let (is_route, method) = check_route_function(&func, source)?;
    if !is_route {
        return None;
    }

    // Get the URL from arguments
    let url = get_first_string_arg(&args, source)?;

    // If using @app.route(), check for methods= argument
    let final_method = if method.is_none() {
        extract_methods_arg(&args, source).or(Some("GET".to_string()))
    } else {
        method
    };

    Some((url, final_method))
}

/// Check if a function node is a route decorator and extract HTTP method.
fn check_route_function(node: &Node, source: &[u8]) -> Option<(bool, Option<String>)> {
    match node.kind() {
        "attribute" => {
            // app.route, app.get, router.post, etc.
            let obj = node.child_by_field_name("object")?;
            let attr = node.child_by_field_name("attribute")?;

            let obj_text = obj.utf8_text(source).ok()?;
            let attr_text = attr.utf8_text(source).ok()?;

            // Known route objects
            let route_objects = ["app", "router", "bp", "blueprint", "api", "route", "routes"];

            // HTTP method decorators
            let method_attrs = [
                ("route", None),
                ("get", Some("GET")),
                ("post", Some("POST")),
                ("put", Some("PUT")),
                ("delete", Some("DELETE")),
                ("patch", Some("PATCH")),
                ("head", Some("HEAD")),
                ("options", Some("OPTIONS")),
            ];

            // Check if object looks like a route object
            let obj_lower = obj_text.to_lowercase();
            let is_route_obj = route_objects.iter().any(|r| obj_lower.contains(r));

            if is_route_obj {
                for (method_name, http_method) in method_attrs {
                    if attr_text == method_name {
                        return Some((true, http_method.map(|s| s.to_string())));
                    }
                }
            }

            None
        }
        "identifier" => {
            // Just @route("/api/users") - less common
            let text = node.utf8_text(source).ok()?;
            if text == "route" {
                Some((true, None))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Get the first string argument from an arguments node.
fn get_first_string_arg(args: &Node, source: &[u8]) -> Option<String> {
    let count = args.child_count();

    for i in 0..count {
        if let Some(child) = args.child(i) {
            match child.kind() {
                "string" => {
                    let text = child.utf8_text(source).ok()?;
                    return Some(strip_quotes(text));
                }
                _ => continue,
            }
        }
    }
    None
}

/// Extract HTTP method from methods= argument: @app.route("/api", methods=["GET", "POST"])
fn extract_methods_arg(args: &Node, source: &[u8]) -> Option<String> {
    let count = args.child_count();

    for i in 0..count {
        if let Some(child) = args.child(i) {
            if child.kind() == "keyword_argument" {
                let name = child.child_by_field_name("name")?;
                let name_text = name.utf8_text(source).ok()?;

                if name_text == "methods" {
                    let value = child.child_by_field_name("value")?;
                    let value_text = value.utf8_text(source).ok()?;

                    // Extract first method from list
                    if value_text.contains("GET") {
                        return Some("GET".to_string());
                    } else if value_text.contains("POST") {
                        return Some("POST".to_string());
                    } else if value_text.contains("PUT") {
                        return Some("PUT".to_string());
                    } else if value_text.contains("DELETE") {
                        return Some("DELETE".to_string());
                    } else if value_text.contains("PATCH") {
                        return Some("PATCH".to_string());
                    }
                }
            }
        }
    }
    None
}

/// Strip quotes from a string.
fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() < 2 {
        return s.to_string();
    }

    // Handle triple quotes
    if s.starts_with("\"\"\"") && s.ends_with("\"\"\"") && s.len() >= 6 {
        return s[3..s.len()-3].to_string();
    }
    if s.starts_with("'''") && s.ends_with("'''") && s.len() >= 6 {
        return s[3..s.len()-3].to_string();
    }

    // Handle f-strings: f"/api/users/{id}"
    let s = if s.starts_with("f\"") || s.starts_with("f'") {
        &s[1..]
    } else {
        s
    };

    let first = s.chars().next().unwrap();
    let last = s.chars().last().unwrap();

    if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
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
            // Python f-string or path param: {id} or {user_id}
            '{' => {
                while let Some(c2) = chars.next() {
                    if c2 == '}' {
                        break;
                    }
                }
                result.push_str(":param");
            }
            // Flask/Werkzeug style: <id> or <int:id>
            '<' => {
                while let Some(c2) = chars.next() {
                    if c2 == '>' {
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
        || (url.starts_with('/') && url.len() > 1 && !url.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url() {
        assert_eq!(normalize_url("/api/users/{id}"), "/api/users/:param");
        assert_eq!(normalize_url("/api/users/<int:id>"), "/api/users/:param");
        assert_eq!(normalize_url("/api/items/<id>/comments/<cid>"), "/api/items/:param/comments/:param");
    }

    #[test]
    fn test_is_api_url() {
        assert!(is_api_url("/api/users"));
        assert!(is_api_url("/v1/products"));
        assert!(is_api_url("/users"));
        assert!(!is_api_url(""));
        assert!(!is_api_url("/static/styles.css"));
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes("\"/api/users\""), "/api/users");
        assert_eq!(strip_quotes("'/api/users'"), "/api/users");
        assert_eq!(strip_quotes("f\"/api/users/{id}\""), "/api/users/{id}");
    }
}

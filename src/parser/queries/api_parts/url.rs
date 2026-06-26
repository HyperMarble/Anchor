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

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

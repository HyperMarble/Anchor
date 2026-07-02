//
//  api.rs
//  Anchor
//
//  Unified API endpoint extractor. One generic walker + pattern tables per language.
//  Replaces all per-language extractors (python.rs, javascript.rs, go.rs, etc.)
//

use std::path::Path;
use tree_sitter::Node;

use crate::parser::language::SupportedLanguage;
use crate::parser::types::{ApiEndpointKind, ExtractedApiEndpoint};

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

include!("api_parts/config_a.rs");
include!("api_parts/config_b.rs");
include!("api_parts/extract.rs");
include!("api_parts/scope.rs");
include!("api_parts/url.rs");

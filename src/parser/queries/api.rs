//
//  api.rs
//  Anchor
//
//  Created by hak (tharun)
//

use std::path::Path;
use tree_sitter::Node;

use crate::graph::types::ExtractedApiEndpoint;
use crate::parser::language::SupportedLanguage;

use super::python::extract_python_apis;
use super::javascript::extract_js_apis;
use super::go::extract_go_apis;
use super::java::extract_java_apis;
use super::csharp::extract_csharp_apis;
use super::ruby::extract_ruby_apis;

/// Extract API endpoints from a parsed AST.
///
/// Dispatches to language-specific extractors that walk the AST directly.
pub fn extract_api_endpoints(
    root: &Node,
    source: &[u8],
    language: SupportedLanguage,
    file_path: &Path,
) -> Vec<ExtractedApiEndpoint> {
    match language {
        SupportedLanguage::Python => {
            extract_python_apis(root, source)
        }
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            let is_likely_backend = is_backend_file(file_path);
            extract_js_apis(root, source, is_likely_backend)
        }
        SupportedLanguage::Go => {
            extract_go_apis(root, source)
        }
        SupportedLanguage::Java => {
            extract_java_apis(root, source)
        }
        SupportedLanguage::CSharp => {
            extract_csharp_apis(root, source)
        }
        SupportedLanguage::Ruby => {
            extract_ruby_apis(root, source)
        }
        // Languages without API detection yet
        SupportedLanguage::Rust | SupportedLanguage::Cpp | SupportedLanguage::Swift => {
            Vec::new()
        }
    }
}

/// Heuristic to determine if a JS/TS file is likely backend code.
fn is_backend_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();

    // Common backend indicators
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
    fn test_is_backend_file() {
        assert!(is_backend_file(&PathBuf::from("/project/server/index.ts")));
        assert!(is_backend_file(&PathBuf::from("/project/api/routes/users.js")));
        assert!(is_backend_file(&PathBuf::from("/project/app.server.ts")));
        assert!(!is_backend_file(&PathBuf::from("/project/src/components/Button.tsx")));
        assert!(!is_backend_file(&PathBuf::from("/project/pages/index.tsx")));
    }
}

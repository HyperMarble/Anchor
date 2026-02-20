//
//  language.rs
//  Anchor
//
//  Created by hak (tharun)
//

use serde::{Deserialize, Serialize};
use std::path::Path;
use tree_sitter::Language;

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SupportedLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    Java,
    CSharp,
    Ruby,
    Cpp,
    Swift,
}

impl SupportedLanguage {
    /// Detect language from file extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        match ext {
            "rs" => Some(SupportedLanguage::Rust),
            "py" | "pyw" => Some(SupportedLanguage::Python),
            "js" | "mjs" | "cjs" => Some(SupportedLanguage::JavaScript),
            "ts" | "mts" | "cts" => Some(SupportedLanguage::TypeScript),
            "tsx" | "jsx" => Some(SupportedLanguage::Tsx),
            "go" => Some(SupportedLanguage::Go),
            "java" => Some(SupportedLanguage::Java),
            "cs" => Some(SupportedLanguage::CSharp),
            "rb" => Some(SupportedLanguage::Ruby),
            // "kt" | "kts" => Some(SupportedLanguage::Kotlin),  // Disabled: tree-sitter version conflict
            "cpp" | "cc" | "cxx" | "hpp" | "h" => Some(SupportedLanguage::Cpp),
            "swift" => Some(SupportedLanguage::Swift),
            _ => None,
        }
    }

    /// Get the tree-sitter Language for this language.
    pub fn tree_sitter_language(&self) -> Language {
        match self {
            SupportedLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
            SupportedLanguage::Python => tree_sitter_python::LANGUAGE.into(),
            SupportedLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            SupportedLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            SupportedLanguage::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            SupportedLanguage::Go => tree_sitter_go::LANGUAGE.into(),
            SupportedLanguage::Java => tree_sitter_java::LANGUAGE.into(),
            SupportedLanguage::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            SupportedLanguage::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            SupportedLanguage::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            SupportedLanguage::Swift => tree_sitter_swift::LANGUAGE.into(),
        }
    }

    /// Get the display name.
    pub fn name(&self) -> &'static str {
        match self {
            SupportedLanguage::Rust => "Rust",
            SupportedLanguage::Python => "Python",
            SupportedLanguage::JavaScript => "JavaScript",
            SupportedLanguage::TypeScript => "TypeScript",
            SupportedLanguage::Tsx => "TSX",
            SupportedLanguage::Go => "Go",
            SupportedLanguage::Java => "Java",
            SupportedLanguage::CSharp => "C#",
            SupportedLanguage::Ruby => "Ruby",
            SupportedLanguage::Cpp => "C++",
            SupportedLanguage::Swift => "Swift",
        }
    }

    /// Check if two languages are in the same ecosystem (can call each other).
    pub fn same_ecosystem(&self, other: &Self) -> bool {
        match (self, other) {
            // JavaScript ecosystem (JS, TS, TSX can all import each other)
            (SupportedLanguage::JavaScript, SupportedLanguage::JavaScript) => true,
            (SupportedLanguage::JavaScript, SupportedLanguage::TypeScript) => true,
            (SupportedLanguage::JavaScript, SupportedLanguage::Tsx) => true,
            (SupportedLanguage::TypeScript, SupportedLanguage::JavaScript) => true,
            (SupportedLanguage::TypeScript, SupportedLanguage::TypeScript) => true,
            (SupportedLanguage::TypeScript, SupportedLanguage::Tsx) => true,
            (SupportedLanguage::Tsx, SupportedLanguage::JavaScript) => true,
            (SupportedLanguage::Tsx, SupportedLanguage::TypeScript) => true,
            (SupportedLanguage::Tsx, SupportedLanguage::Tsx) => true,

            // JVM ecosystem
            (SupportedLanguage::Java, SupportedLanguage::Java) => true,

            // Each other language is its own ecosystem
            (SupportedLanguage::Python, SupportedLanguage::Python) => true,
            (SupportedLanguage::Rust, SupportedLanguage::Rust) => true,
            (SupportedLanguage::Go, SupportedLanguage::Go) => true,
            (SupportedLanguage::CSharp, SupportedLanguage::CSharp) => true,
            (SupportedLanguage::Ruby, SupportedLanguage::Ruby) => true,
            (SupportedLanguage::Cpp, SupportedLanguage::Cpp) => true,
            (SupportedLanguage::Swift, SupportedLanguage::Swift) => true,

            // Cross-ecosystem: no direct calls possible (but APIs can connect them!)
            _ => false,
        }
    }
}

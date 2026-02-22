//
//  builder.rs
//  Anchor
//
//  Created by hak (tharun)
//

use ignore::WalkBuilder;
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use super::engine::CodeGraph;
use super::types::FileExtractions;
use crate::parser::{extract_file, SupportedLanguage};

/// Directories that should never be indexed, even without .gitignore.
const BUILTIN_IGNORE: &[&str] = &[
    "node_modules",
    "vendor",
    "dist",
    "build",
    ".git",
    ".svn",
    ".hg",
    "__pycache__",
    ".tox",
    ".venv",
    "venv",
    "env",
    ".env",
    "target",
    ".next",
    ".nuxt",
    "coverage",
    ".cache",
    ".turbo",
    ".output",
    "pkg",
    "bin",
];

/// Check if a path contains any built-in ignored directory.
fn is_builtin_ignored(path: &Path) -> bool {
    path.components().any(|c| {
        if let std::path::Component::Normal(name) = c {
            BUILTIN_IGNORE.contains(&name.to_str().unwrap_or(""))
        } else {
            false
        }
    })
}

/// Build a code graph from all source files in a directory.
///
/// Respects .gitignore, walks recursively, parses all supported
/// language files, and returns a fully connected CodeGraph.
pub fn build_graph(roots: &[&Path]) -> CodeGraph {
    let files: Vec<_> = roots
        .iter()
        .flat_map(|root| {
            WalkBuilder::new(root)
                .hidden(true)
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .add_custom_ignore_filename(".anchorignore")
                .build()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
                .filter(|entry| !is_builtin_ignored(entry.path()))
                .filter(|entry| SupportedLanguage::from_path(entry.path()).is_some())
                .map(|entry| entry.into_path())
        })
        .collect();
    let extractions: Mutex<Vec<FileExtractions>> = Mutex::new(Vec::with_capacity(files.len()));

    files.par_iter().for_each(|file_path| {
        if let Ok(source) = fs::read_to_string(file_path) {
            if let Ok(extraction) = extract_file(file_path, &source) {
                if let Ok(mut exts) = extractions.lock() {
                    exts.push(extraction);
                }
            }
        }
    });

    let extractions = extractions.into_inner().unwrap_or_default();

    let mut graph = CodeGraph::new();
    graph.build_from_extractions(extractions);

    graph
}

pub fn rebuild_file(
    graph: &mut CodeGraph,
    file_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let source = fs::read_to_string(file_path)?;
    let extraction = extract_file(file_path, &source)?;
    graph.update_file_incremental(file_path, extraction);
    Ok(())
}

/// Get statistics about what files would be parsed in a directory.
pub fn scan_stats(root: &Path) -> ScanStats {
    let mut stats = ScanStats::default();

    for entry in WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .add_custom_ignore_filename(".anchorignore")
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
        .filter(|e| !is_builtin_ignored(e.path()))
    {
        if let Some(lang) = SupportedLanguage::from_path(entry.path()) {
            stats.total_files += 1;
            match lang {
                SupportedLanguage::Rust => stats.rust_files += 1,
                SupportedLanguage::Python => stats.python_files += 1,
                SupportedLanguage::JavaScript => stats.js_files += 1,
                SupportedLanguage::TypeScript | SupportedLanguage::Tsx => stats.ts_files += 1,
                _ => {}
            }
        }
    }

    stats
}

#[derive(Debug, Clone, Default)]
pub struct ScanStats {
    pub total_files: usize,
    pub rust_files: usize,
    pub python_files: usize,
    pub js_files: usize,
    pub ts_files: usize,
}

impl std::fmt::Display for ScanStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Found {} source files (Rust: {}, Python: {}, JS: {}, TS: {})",
            self.total_files, self.rust_files, self.python_files, self.js_files, self.ts_files
        )
    }
}

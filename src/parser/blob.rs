//
//  blob.rs
//  Anchor
//
//  Universal fallback extractor for files tree-sitter doesn't support.
//  Handles markdown, CSV, config files, and any unknown text file.
//

use crate::graph::types::*;
use std::path::Path;

/// Extract symbols from any file tree-sitter can't parse.
/// Returns None only if the file is binary (non-UTF-8).
pub fn extract_blob(path: &Path, source: &str) -> Option<FileExtractions> {
    if source.is_empty() {
        return None;
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let symbols = match ext {
        "md" | "mdx" => extract_md(source),
        "csv" => extract_csv(source),
        "toml" | "yaml" | "yml" | "json" => extract_config(source, path),
        // Unknown text file — try brace-based, fall back to whole-file chunk
        _ => {
            let chunks = extract_brace(source);
            if chunks.is_empty() {
                whole_file_chunk(source, path)
            } else {
                chunks
            }
        }
    };

    if symbols.is_empty() {
        return None;
    }

    Some(FileExtractions {
        file_path: path.to_path_buf(),
        symbols,
        imports: vec![],
        calls: vec![],
        api_endpoints: vec![],
    })
}

// ── Markdown ──────────────────────────────────────────────────────────────────

fn extract_md(source: &str) -> Vec<ExtractedSymbol> {
    let lines: Vec<&str> = source.lines().collect();
    let mut symbols = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].starts_with('#') {
            let name = lines[i].trim_start_matches('#').trim().to_string();
            if name.is_empty() {
                i += 1;
                continue;
            }

            let start = i;
            let end = lines[i + 1..]
                .iter()
                .position(|l| l.starts_with('#'))
                .map(|p| i + p)
                .unwrap_or(lines.len().saturating_sub(1));

            let snippet = lines[start..=end].join("\n");
            symbols.push(ExtractedSymbol {
                name,
                kind: NodeKind::Module,
                line_start: start + 1,
                line_end: end + 1,
                code_snippet: snippet,
                parent: None,
                features: vec!["doc".into(), "markdown".into()],
            });
            i = end;
        }
        i += 1;
    }
    symbols
}

// ── CSV ───────────────────────────────────────────────────────────────────────

fn extract_csv(source: &str) -> Vec<ExtractedSymbol> {
    source
        .lines()
        .enumerate()
        .skip(1)
        .filter_map(|(i, line)| {
            let name = if line.starts_with('"') {
                line.trim_start_matches('"').split('"').next()?.to_string()
            } else {
                line.split(',').next()?.trim().to_string()
            };
            if name.is_empty() {
                return None;
            }
            Some(ExtractedSymbol {
                name,
                kind: NodeKind::Variable,
                line_start: i + 1,
                line_end: i + 1,
                code_snippet: line.to_string(),
                parent: None,
                features: vec!["data".into(), "csv".into()],
            })
        })
        .collect()
}

// ── Config files ──────────────────────────────────────────────────────────────

fn extract_config(source: &str, path: &Path) -> Vec<ExtractedSymbol> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config")
        .to_string();

    vec![ExtractedSymbol {
        name,
        kind: NodeKind::Module,
        line_start: 1,
        line_end: source.lines().count(),
        code_snippet: source.to_string(),
        parent: None,
        features: vec!["config".into()],
    }]
}

// ── Brace-based (unknown code-like files) ────────────────────────────────────

const DECL_KEYWORDS: &[&str] = &[
    "pub async fn ",
    "pub fn ",
    "async fn ",
    "fn ",
    "pub struct ",
    "struct ",
    "pub enum ",
    "enum ",
    "pub impl ",
    "impl ",
    "pub trait ",
    "trait ",
    "def ",
    "async def ",
    "class ",
    "function ",
];

fn sanitize(line: &str) -> String {
    let mut out = String::new();
    let mut in_str = false;
    let mut str_ch = '"';
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_str {
            if ch == '\\' {
                chars.next();
                continue;
            }
            if ch == str_ch {
                in_str = false;
            }
        } else {
            match ch {
                '"' => {
                    in_str = true;
                    str_ch = '"';
                }
                '/' if chars.peek() == Some(&'/') => break,
                '#' => break,
                _ => out.push(ch),
            }
        }
    }
    out
}

fn extract_brace(source: &str) -> Vec<ExtractedSymbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        let kw = DECL_KEYWORDS.iter().find(|&&kw| trimmed.starts_with(kw));

        if let Some(kw) = kw {
            let name: String = trimmed[kw.len()..]
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("")
                .to_string();

            if name.is_empty() {
                i += 1;
                continue;
            }

            let start = i;
            let mut depth = 0i32;
            let mut end = i;
            let mut found_open = false;

            for (j, line) in lines[i..].iter().enumerate() {
                let clean = sanitize(line);
                for ch in clean.chars() {
                    if ch == '{' {
                        depth += 1;
                        found_open = true;
                    }
                    if ch == '}' {
                        depth -= 1;
                    }
                }
                end = i + j;
                if found_open && depth <= 0 {
                    break;
                }
            }

            let snippet = lines[start..=end].join("\n");
            symbols.push(ExtractedSymbol {
                name,
                kind: NodeKind::Function,
                line_start: start + 1,
                line_end: end + 1,
                code_snippet: snippet,
                parent: None,
                features: vec![],
            });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    symbols
}

fn whole_file_chunk(source: &str, path: &Path) -> Vec<ExtractedSymbol> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("__file__")
        .to_string();
    vec![ExtractedSymbol {
        name,
        kind: NodeKind::Module,
        line_start: 1,
        line_end: source.lines().count(),
        code_snippet: source.to_string(),
        parent: None,
        features: vec![],
    }]
}

//
//  schema.rs
//  Anchor
//
//  Created by hak (tharun)
//

use async_graphql::{ComplexObject, Context, Result, SimpleObject};
use std::sync::Arc;

use crate::graph::CodeGraph;
use crate::query::slice::slice_code;

/// A code symbol (function, class, struct, etc.)
#[derive(SimpleObject)]
#[graphql(complex)]
pub struct Symbol {
    /// Symbol name
    pub name: String,
    /// Kind: function, class, struct, method, etc.
    pub kind: String,
    /// File path
    pub file: String,
    /// Line number
    pub line: i32,
    /// Source code (only if requested)
    #[graphql(skip)]
    pub code_internal: Option<String>,
    /// Call lines for graph slicing
    #[graphql(skip)]
    pub call_lines: Vec<usize>,
}

#[ComplexObject]
impl Symbol {
    /// Source code of the symbol — graph-sliced to show only dependency-relevant lines.
    /// When sliced, prepends a coverage indicator: [25/88 lines, 3 calls].
    /// Set full=true to disable slicing and get complete code with line numbers.
    async fn code(&self, #[graphql(default = false)] full: bool) -> Option<String> {
        self.code_internal.as_ref().map(|code| {
            if full || self.call_lines.is_empty() {
                add_line_numbers(code, self.line as usize)
            } else {
                let result = slice_code(code, &self.call_lines, self.line as usize);
                if result.was_sliced {
                    format!(
                        "[{}/{} lines, {} calls]\n{}",
                        result.shown_lines, result.total_lines, result.call_count, result.code
                    )
                } else {
                    result.code
                }
            }
        })
    }

    /// Symbols that call/use this symbol
    async fn callers(&self, ctx: &Context<'_>) -> Result<Vec<Symbol>> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let deps = graph.dependents(&self.name);
        Ok(deps
            .into_iter()
            .take(20) // Limit to prevent explosion
            .map(|d| Symbol {
                name: d.symbol,
                kind: d.kind.to_string(),
                file: d.file.to_string_lossy().to_string(),
                line: d.line as i32,
                code_internal: None,
                call_lines: vec![],
            })
            .collect())
    }

    /// Symbols this symbol calls/uses
    async fn callees(&self, ctx: &Context<'_>) -> Result<Vec<Symbol>> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let deps = graph.dependencies(&self.name);
        Ok(deps
            .into_iter()
            .take(20)
            .map(|d| Symbol {
                name: d.symbol,
                kind: d.kind.to_string(),
                file: d.file.to_string_lossy().to_string(),
                line: d.line as i32,
                code_internal: None,
                call_lines: vec![],
            })
            .collect())
    }
}

/// File with its symbols
#[derive(SimpleObject)]
#[graphql(complex)]
pub struct File {
    /// File path
    pub path: String,
    /// Whether the file was found
    pub found: bool,
}

#[ComplexObject]
impl File {
    /// Symbols defined in this file
    async fn symbols(&self, ctx: &Context<'_>) -> Result<Vec<Symbol>> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let symbols = graph.symbols_in_file(std::path::Path::new(&self.path));
        Ok(symbols
            .into_iter()
            .map(|s| Symbol {
                name: s.name.clone(),
                kind: s.kind.to_string(),
                file: s.file_path.to_string_lossy().to_string(),
                line: s.line_start as i32,
                code_internal: Some(s.code_snippet.clone()),
                call_lines: s.call_lines.clone(),
            })
            .collect())
    }
}

/// Graph statistics
#[derive(SimpleObject)]
pub struct Stats {
    /// Number of files indexed
    pub files: i32,
    /// Number of symbols extracted
    pub symbols: i32,
    /// Number of relationships (edges)
    pub edges: i32,
}

/// Result of a write operation
#[derive(SimpleObject)]
pub struct WriteResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// File that was modified
    pub file: Option<String>,
    /// Line number of modification
    pub line: Option<i32>,
    /// Error message if failed
    pub error: Option<String>,
}

impl WriteResult {
    pub fn ok(file: &str, line: usize) -> Self {
        Self {
            success: true,
            file: Some(file.to_string()),
            line: Some(line as i32),
            error: None,
        }
    }

    pub fn err(msg: &str) -> Self {
        Self {
            success: false,
            file: None,
            line: None,
            error: Some(msg.to_string()),
        }
    }
}

/// Add line numbers to full (unsliced) code for consistent output format.
fn add_line_numbers(code: &str, line_start: usize) -> String {
    let mut result = String::new();
    for (i, line) in code.lines().enumerate() {
        result.push_str(&format!("{:>4}: {}\n", line_start + i, line));
    }
    result
}

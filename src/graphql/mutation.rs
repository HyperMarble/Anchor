//
//  mutation.rs
//  Anchor
//
//  Created by hak (tharun)
//

use async_graphql::{Context, Object, Result};
use std::path::Path;
use std::sync::Arc;

use super::schema::WriteResult;
use crate::graph::CodeGraph;
use crate::write;

/// Root mutation type
pub struct Mutation;

/// Look up a symbol and run a write operation against its file/code.
fn with_symbol<F>(graph: &CodeGraph, symbol: &str, write_fn: F) -> WriteResult
where
    F: FnOnce(&Path, &str) -> Result<crate::write::WriteResult, crate::write::WriteError>,
{
    let results = graph.search(symbol, 1);
    if results.is_empty() {
        return WriteResult::err(&format!("Symbol '{}' not found", symbol));
    }
    let sym = &results[0];
    let file = sym.file.to_string_lossy().to_string();
    match write_fn(Path::new(&file), &sym.code) {
        Ok(r) => WriteResult::ok(&file, r.lines_written),
        Err(e) => WriteResult::err(&e.to_string()),
    }
}

#[Object]
impl Mutation {
    /// Create a new file with content
    async fn create_file(&self, path: String, content: String) -> Result<WriteResult> {
        match write::create_file(Path::new(&path), &content) {
            Ok(r) => Ok(WriteResult::ok(&path, r.lines_written)),
            Err(e) => Ok(WriteResult::err(&e.to_string())),
        }
    }

    /// Insert code after a symbol (uses symbol's code as pattern)
    async fn insert_after(
        &self,
        ctx: &Context<'_>,
        symbol: String,
        code: String,
    ) -> Result<WriteResult> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        Ok(with_symbol(graph, &symbol, |file, pattern| {
            write::insert_after(file, pattern, &code)
        }))
    }

    /// Insert code before a symbol (uses symbol's code as pattern)
    async fn insert_before(
        &self,
        ctx: &Context<'_>,
        symbol: String,
        code: String,
    ) -> Result<WriteResult> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        Ok(with_symbol(graph, &symbol, |file, pattern| {
            write::insert_before(file, pattern, &code)
        }))
    }

    /// Replace a symbol's code entirely
    async fn replace_symbol(
        &self,
        ctx: &Context<'_>,
        symbol: String,
        new_code: String,
    ) -> Result<WriteResult> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        Ok(with_symbol(graph, &symbol, |file, pattern| {
            write::replace_first(file, pattern, &new_code)
        }))
    }

    /// Replace all occurrences of a pattern in a file
    async fn replace_all(
        &self,
        path: String,
        pattern: String,
        replacement: String,
    ) -> Result<WriteResult> {
        match write::replace_all(Path::new(&path), &pattern, &replacement) {
            Ok(r) => Ok(WriteResult::ok(&path, r.lines_written)),
            Err(e) => Ok(WriteResult::err(&e.to_string())),
        }
    }
}

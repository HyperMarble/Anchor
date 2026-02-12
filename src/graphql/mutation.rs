//! GraphQL Mutation resolvers.
//!
//! Write operations for code modification.

use async_graphql::{Context, Object, Result};
use std::path::Path;
use std::sync::Arc;

use super::schema::WriteResult;
use crate::graph::CodeGraph;
use crate::write;

/// Root mutation type
pub struct Mutation;

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

        // Find the symbol
        let results = graph.search(&symbol, 1);
        if results.is_empty() {
            return Ok(WriteResult::err(&format!("Symbol '{}' not found", symbol)));
        }

        let sym = &results[0];
        let file = sym.file.to_string_lossy().to_string();

        // Use the symbol's code as the pattern to insert after
        match write::insert_after(Path::new(&file), &sym.code, &code) {
            Ok(r) => Ok(WriteResult::ok(&file, r.lines_written)),
            Err(e) => Ok(WriteResult::err(&e.to_string())),
        }
    }

    /// Insert code before a symbol (uses symbol's code as pattern)
    async fn insert_before(
        &self,
        ctx: &Context<'_>,
        symbol: String,
        code: String,
    ) -> Result<WriteResult> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;

        let results = graph.search(&symbol, 1);
        if results.is_empty() {
            return Ok(WriteResult::err(&format!("Symbol '{}' not found", symbol)));
        }

        let sym = &results[0];
        let file = sym.file.to_string_lossy().to_string();

        match write::insert_before(Path::new(&file), &sym.code, &code) {
            Ok(r) => Ok(WriteResult::ok(&file, r.lines_written)),
            Err(e) => Ok(WriteResult::err(&e.to_string())),
        }
    }

    /// Replace a symbol's code entirely
    async fn replace_symbol(
        &self,
        ctx: &Context<'_>,
        symbol: String,
        new_code: String,
    ) -> Result<WriteResult> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;

        let results = graph.search(&symbol, 1);
        if results.is_empty() {
            return Ok(WriteResult::err(&format!("Symbol '{}' not found", symbol)));
        }

        let sym = &results[0];
        let file = sym.file.to_string_lossy().to_string();

        // Replace the symbol's code with new code
        match write::replace_first(Path::new(&file), &sym.code, &new_code) {
            Ok(r) => Ok(WriteResult::ok(&file, r.lines_written)),
            Err(e) => Ok(WriteResult::err(&e.to_string())),
        }
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

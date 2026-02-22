//
//  query.rs
//  Anchor
//
//  Created by hak (tharun)
//

use async_graphql::{Context, Object, Result};
use std::sync::Arc;

use super::schema::{File, Stats, Symbol};
use crate::graph::CodeGraph;
use crate::regex::{parse, Matcher};

/// Root query type
pub struct Query;

#[Object]
impl Query {
    /// Search for symbols by name or regex pattern.
    ///
    /// Three modes:
    /// - `exact: true` - only exact matches
    /// - `pattern` - regex pattern (ReDoS-safe, supports & intersection, ~ negation)
    /// - default - prefix matching
    ///
    /// Regex examples:
    /// - `Config.*Manager` - starts with Config, ends with Manager
    /// - `.*Service` - ends with Service
    /// - `get.*&.*User` - contains "get" AND "User"
    async fn symbol(
        &self,
        ctx: &Context<'_>,
        name: String,
        #[graphql(default = false)] exact: bool,
        #[graphql(default)] pattern: Option<String>,
    ) -> Result<Vec<Symbol>> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let results = graph.search(&name, 50); // Get more for pattern filtering

        let filtered: Vec<_> = if let Some(ref pat) = pattern {
            // Use Brzozowski derivatives regex - ReDoS-safe, case-insensitive
            let regex =
                parse(&pat.to_lowercase()).map_err(|e| async_graphql::Error::new(e.to_string()))?;
            let mut matcher = Matcher::new(regex);
            results
                .into_iter()
                .filter(|r| matcher.is_match(&r.symbol.to_lowercase()))
                .collect()
        } else if exact {
            results.into_iter().filter(|r| r.symbol == name).collect()
        } else {
            results
                .into_iter()
                .filter(|r| r.symbol.starts_with(&name) || r.symbol == name)
                .collect()
        };

        Ok(filtered
            .into_iter()
            .take(10)
            .map(|r| Symbol {
                name: r.symbol,
                kind: r.kind.to_string(),
                file: r.file.to_string_lossy().to_string(),
                line: r.line_start as i32,
                code_internal: Some(r.code),
                call_lines: r.call_lines,
                features: r.features,
            })
            .collect())
    }

    /// Get a file and its symbols
    async fn file(&self, ctx: &Context<'_>, path: String) -> Result<File> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let symbols = graph.symbols_in_file(std::path::Path::new(&path));
        Ok(File {
            path,
            found: !symbols.is_empty(),
        })
    }

    /// Get symbols that depend on the given symbol (callers)
    async fn dependents(&self, ctx: &Context<'_>, symbol: String) -> Result<Vec<Symbol>> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let deps = graph.dependents(&symbol);
        Ok(deps
            .into_iter()
            .take(50)
            .map(|d| Symbol {
                name: d.symbol,
                kind: d.kind.to_string(),
                file: d.file.to_string_lossy().to_string(),
                line: d.line as i32,
                code_internal: None,
                call_lines: vec![],
                features: vec![],
            })
            .collect())
    }

    /// Get symbols that this symbol depends on (callees)
    async fn dependencies(&self, ctx: &Context<'_>, symbol: String) -> Result<Vec<Symbol>> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let deps = graph.dependencies(&symbol);
        Ok(deps
            .into_iter()
            .take(50)
            .map(|d| Symbol {
                name: d.symbol,
                kind: d.kind.to_string(),
                file: d.file.to_string_lossy().to_string(),
                line: d.line as i32,
                code_internal: None,
                call_lines: vec![],
                features: vec![],
            })
            .collect())
    }

    /// Get graph statistics
    async fn stats(&self, ctx: &Context<'_>) -> Result<Stats> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let s = graph.stats();
        Ok(Stats {
            files: s.file_count as i32,
            symbols: s.symbol_count as i32,
            edges: s.total_edges as i32,
        })
    }

    /// Search all symbols with a regex pattern.
    ///
    /// Uses Brzozowski derivatives - ReDoS-safe, O(n) time complexity.
    /// Supports boolean algebra:
    /// - `R1|R2` - union (match either)
    /// - `R1&R2` - intersection (match both)
    /// - `~R` - negation (match anything except)
    ///
    /// Examples:
    /// - `Config.*` - symbols starting with "Config"
    /// - `.*Manager` - symbols ending with "Manager"
    /// - `Config.*&.*Manager` - starts with "Config" AND ends with "Manager"
    /// - `[A-Z][a-z]+` - CamelCase words
    async fn search(
        &self,
        ctx: &Context<'_>,
        pattern: String,
        #[graphql(default = 20)] limit: i32,
    ) -> Result<Vec<Symbol>> {
        let graph = ctx.data::<Arc<CodeGraph>>()?;
        let regex =
            parse(&pattern.to_lowercase()).map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let mut matcher = Matcher::new(regex);

        // Get all symbols from the graph and filter with regex (case-insensitive)
        let all_symbols = graph.all_symbols();
        let matched: Vec<_> = all_symbols
            .iter()
            .filter(|r| matcher.is_match(&r.symbol.to_lowercase()))
            .take(limit as usize)
            .map(|r| Symbol {
                name: r.symbol.clone(),
                kind: r.kind.to_string(),
                file: r.file.to_string_lossy().to_string(),
                line: r.line_start as i32,
                code_internal: Some(r.code.clone()),
                call_lines: r.call_lines.clone(),
                features: r.features.clone(),
            })
            .collect();

        // If regex found nothing, fall back to feature-based search
        if matched.is_empty() {
            let query_terms: Vec<&str> = pattern
                .split(&['*', '.', ' '][..])
                .filter(|t| t.len() > 2)
                .collect();
            if !query_terms.is_empty() {
                let mut scored: Vec<(usize, &_)> = all_symbols
                    .iter()
                    .filter(|r| !r.features.is_empty())
                    .filter_map(|r| {
                        let hits = query_terms
                            .iter()
                            .filter(|t| {
                                r.features
                                    .iter()
                                    .any(|f| f.contains(t.to_lowercase().as_str()))
                            })
                            .count();
                        if hits > 0 {
                            Some((query_terms.len() - hits, r))
                        } else {
                            None
                        }
                    })
                    .collect();
                scored.sort_by_key(|(score, _)| *score);
                return Ok(scored
                    .into_iter()
                    .take(limit as usize)
                    .map(|(_, r)| Symbol {
                        name: r.symbol.clone(),
                        kind: r.kind.to_string(),
                        file: r.file.to_string_lossy().to_string(),
                        line: r.line_start as i32,
                        code_internal: Some(r.code.clone()),
                        call_lines: r.call_lines.clone(),
                        features: r.features.clone(),
                    })
                    .collect());
            }
        }

        Ok(matched)
    }
}

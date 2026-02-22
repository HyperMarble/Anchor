//
//  types.rs
//  Anchor
//
//  Created by hak (tharun)
//

use serde::{Deserialize, Serialize};

use crate::graph::{DependencyInfo, GraphStats, SearchResult};

/// Query input — supports both simple string and structured queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Query {
    /// Simple string query: "login"
    Simple(String),
    /// Structured query with optional filters.
    Structured {
        symbol: String,
        kind: Option<String>,
        file: Option<String>,
    },
}

impl Query {
    pub fn symbol_name(&self) -> &str {
        match self {
            Query::Simple(s) => s.as_str(),
            Query::Structured { symbol, .. } => symbol.as_str(),
        }
    }
}

// ─── Basic Search Response ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub found: bool,
    pub count: usize,
    pub results: Vec<SearchResult>,
}

// ─── Dependency Response ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyResponse {
    pub symbol: String,
    pub dependents: Vec<DependencyInfo>,
    pub dependencies: Vec<DependencyInfo>,
}

// ─── Stats Response ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub stats: GraphStats,
}

// ─── File Symbols Response ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbolsResponse {
    pub file: String,
    pub found: bool,
    pub symbols: Vec<FileSymbolEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbolEntry {
    pub name: String,
    pub kind: String,
    pub line_start: usize,
    pub line_end: usize,
    pub code: String,
}

// ─── Context Response (The Main One) ───────────────────────────────

/// The unified context response for AI agents.
///
/// Three intents:
/// - `explore`: Understand what something is and how it connects
/// - `change`: Modify something - shows what will break and how to fix
/// - `create`: Add something similar - shows patterns to follow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    /// The query that was executed
    pub query: String,
    /// The intent: explore, change, create
    pub intent: String,
    /// Whether the query found results
    pub found: bool,

    /// The symbol(s) found
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<Symbol>,

    /// What uses this (callers, importers) - for explore/change
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub used_by: Vec<Reference>,

    /// What this uses (callees, imports) - for explore
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<Reference>,

    /// Suggested edits for dependents - for change
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<Edit>,

    /// Similar patterns to follow - for create
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<Symbol>,

    /// Related tests - for change (to know what to update)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<Symbol>,

    /// Project/file overview stats
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<GraphStats>,
}

impl Default for ContextResponse {
    fn default() -> Self {
        Self {
            query: String::new(),
            intent: "explore".to_string(),
            found: false,
            symbols: Vec::new(),
            used_by: Vec::new(),
            uses: Vec::new(),
            edits: Vec::new(),
            patterns: Vec::new(),
            tests: Vec::new(),
            stats: None,
        }
    }
}

/// A code symbol with its location and source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
    pub code: String,
}

impl Symbol {
    pub fn from_search_result(r: &SearchResult) -> Self {
        Self {
            name: r.symbol.clone(),
            kind: r.kind.to_string(),
            file: r.file.to_string_lossy().to_string(),
            line: r.line_start,
            code: r.code.clone(),
        }
    }
}

/// A reference to another symbol (lighter than full Symbol).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
    /// How it's related: "calls", "imports", "references"
    pub relationship: String,
}

impl Reference {
    pub fn from_dep(dep: &DependencyInfo) -> Self {
        Self {
            name: dep.symbol.clone(),
            kind: dep.kind.to_string(),
            file: dep.file.to_string_lossy().to_string(),
            line: dep.line,
            relationship: dep.relationship.to_string(),
        }
    }
}

/// A suggested edit when modifying a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edit {
    /// File to edit
    pub file: String,
    /// Line number where the usage occurs
    pub line: usize,
    /// The function/symbol containing the usage
    pub in_symbol: String,
    /// The actual usage expression (e.g., "validate(input)")
    pub usage: String,
    /// The full line of code containing the usage
    pub line_content: String,
    /// Suggested replacement (if new_signature provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested: Option<String>,
    /// New arguments that need values (if signature changed)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub new_args: Vec<String>,
    /// Arguments that were removed
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed_args: Vec<String>,
    /// Context: lines before and after
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<String>,
}

/// Parsed function signature for comparison.
#[derive(Debug, Clone, Default)]
pub struct Signature {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<String>,
}

/// A function parameter.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub typ: String,
}

impl Signature {
    /// Parse a signature string like "validate(input: &str, strict: bool) -> bool"
    pub fn parse(sig: &str) -> Option<Self> {
        let sig = sig.trim();

        // Find function name (before the opening paren)
        let paren_idx = sig.find('(')?;
        let name = sig[..paren_idx].trim();
        // Handle "fn name" or just "name"
        let name = name.strip_prefix("fn ").unwrap_or(name).trim().to_string();

        // Find params (between parens)
        let close_paren = sig.rfind(')')?;
        let params_str = &sig[paren_idx + 1..close_paren];

        let mut params = Vec::new();
        if !params_str.trim().is_empty() {
            for param in params_str.split(',') {
                let param = param.trim();
                if param.is_empty() {
                    continue;
                }
                // Parse "name: type" or just "name"
                if let Some(colon_idx) = param.find(':') {
                    let name = param[..colon_idx].trim().to_string();
                    let typ = param[colon_idx + 1..].trim().to_string();
                    params.push(Param { name, typ });
                } else {
                    params.push(Param {
                        name: param.to_string(),
                        typ: String::new(),
                    });
                }
            }
        }

        // Find return type (after ->)
        let return_type = if close_paren + 1 < sig.len() {
            let after_paren = &sig[close_paren + 1..];
            after_paren
                .find("->")
                .map(|arrow_idx| after_paren[arrow_idx + 2..].trim().to_string())
        } else {
            None
        };

        Some(Signature {
            name,
            params,
            return_type,
        })
    }

    /// Compare with another signature and return (added_params, removed_params)
    pub fn diff(&self, new: &Signature) -> (Vec<Param>, Vec<Param>) {
        let old_names: std::collections::HashSet<_> = self.params.iter().map(|p| &p.name).collect();
        let new_names: std::collections::HashSet<_> = new.params.iter().map(|p| &p.name).collect();

        let added: Vec<Param> = new
            .params
            .iter()
            .filter(|p| !old_names.contains(&p.name))
            .cloned()
            .collect();

        let removed: Vec<Param> = self
            .params
            .iter()
            .filter(|p| !new_names.contains(&p.name))
            .cloned()
            .collect();

        (added, removed)
    }
}

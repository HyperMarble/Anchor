//
//  types.rs
//  Anchor
//
//  Created by hak (tharun)
//

use rmcp::schemars;
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ContextRequest {
    #[schemars(
        description = "Symbol names to get context for (e.g. [\"login\", \"UserService\"])"
    )]
    pub symbols: Vec<String>,

    #[schemars(description = "Max results per symbol (default: 5)")]
    pub limit: Option<usize>,

    #[schemars(
        description = "Show full unsliced code (default: false). Use when you need every line, not just dependency-relevant ones."
    )]
    pub full: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchRequest {
    #[schemars(description = "Symbol name to search for")]
    pub query: String,

    #[schemars(
        description = "Regex pattern for advanced search (Brzozowski derivatives, ReDoS-safe)"
    )]
    pub pattern: Option<String>,

    #[schemars(description = "Max results (default: 20)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MapRequest {
    #[schemars(description = "Optional scope to zoom into (e.g. \"src/graph\" or \"auth\")")]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteRequest {
    #[schemars(description = "Relative file path (e.g. \"src/main.rs\")")]
    pub path: String,

    #[schemars(description = "Start line (1-indexed, inclusive)")]
    pub start_line: usize,

    #[schemars(description = "End line (1-indexed, inclusive)")]
    pub end_line: usize,

    #[schemars(description = "New code to replace the line range with")]
    pub new_content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImpactRequest {
    #[schemars(
        description = "Symbol name to analyze impact for (e.g. \"login\", \"UserService\")"
    )]
    pub symbol: String,

    #[schemars(
        description = "Optional new signature if you're changing the function (e.g. \"fn login(user: &str, token: &str) -> Result<bool>\")"
    )]
    pub new_signature: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OrderedWriteRequest {
    #[schemars(description = "List of write operations with paths, content, and dependencies")]
    pub operations: Vec<WriteOpRequest>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteOpRequest {
    #[schemars(description = "Relative file path (e.g. \"src/auth.rs\")")]
    pub path: String,

    #[schemars(description = "File content to write")]
    pub content: String,

    #[schemars(
        description = "Symbol name this file defines (e.g. \"AuthService\"). Used to determine write order from existing graph."
    )]
    pub symbol: Option<String>,
}

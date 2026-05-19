//
//  types.rs
//  Anchor
//
//  Created by hak (tharun)
//

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// The kind of a code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File,
    Function,
    Method,
    Struct,
    Class,
    Interface,
    Enum,
    Type,
    Constant,
    Module,
    Import,
    Trait,
    Impl,
    Variable,
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeKind::File => write!(f, "file"),
            NodeKind::Function => write!(f, "function"),
            NodeKind::Method => write!(f, "method"),
            NodeKind::Struct => write!(f, "struct"),
            NodeKind::Class => write!(f, "class"),
            NodeKind::Interface => write!(f, "interface"),
            NodeKind::Enum => write!(f, "enum"),
            NodeKind::Type => write!(f, "type"),
            NodeKind::Constant => write!(f, "constant"),
            NodeKind::Module => write!(f, "module"),
            NodeKind::Import => write!(f, "import"),
            NodeKind::Trait => write!(f, "trait"),
            NodeKind::Impl => write!(f, "impl"),
            NodeKind::Variable => write!(f, "variable"),
        }
    }
}

/// A symbol extracted from parsing a source file.
#[derive(Debug, Clone)]
pub struct ExtractedSymbol {
    pub name: String,
    pub kind: NodeKind,
    pub line_start: usize,
    pub line_end: usize,
    pub code_snippet: String,
    pub parent: Option<String>,
    pub features: Vec<String>,
}

/// An import extracted from a source file.
#[derive(Debug, Clone)]
pub struct ExtractedImport {
    pub path: String,
    pub symbols: Vec<String>,
    pub line: usize,
}

/// A function call extracted from a source file.
#[derive(Debug, Clone)]
pub struct ExtractedCall {
    pub callee: String,
    pub caller: String,
    pub line: usize,
    pub line_end: usize,
}

/// Whether an API endpoint is defined (server) or consumed (client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiEndpointKind {
    Defines,
    Consumes,
}

/// An API endpoint extracted from source code.
#[derive(Debug, Clone)]
pub struct ExtractedApiEndpoint {
    pub url: String,
    pub method: Option<String>,
    pub kind: ApiEndpointKind,
    pub scope: Option<String>,
    pub line: usize,
}

/// All extracted information from a single source file.
#[derive(Debug, Clone)]
pub struct FileExtractions {
    pub file_path: PathBuf,
    pub symbols: Vec<ExtractedSymbol>,
    pub imports: Vec<ExtractedImport>,
    pub calls: Vec<ExtractedCall>,
    pub api_endpoints: Vec<ExtractedApiEndpoint>,
}

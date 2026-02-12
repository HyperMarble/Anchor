//! Core types for the Anchor code graph.
//!
//! Defines node kinds, edge kinds, and the data structures
//! that represent code elements and their relationships.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// The kind of a node in the code graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A source file.
    File,
    /// A function definition.
    Function,
    /// A method (function inside a class/struct/impl).
    Method,
    /// A struct definition (Rust, Go).
    Struct,
    /// A class definition (Python, JS/TS, Java).
    Class,
    /// An interface definition (TS, Go, Java).
    Interface,
    /// An enum definition.
    Enum,
    /// A type alias or type definition.
    Type,
    /// A constant or static variable.
    Constant,
    /// A module or namespace.
    Module,
    /// An import/use statement.
    Import,
    /// A trait definition (Rust).
    Trait,
    /// An impl block (Rust).
    Impl,
    /// A variable or field.
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

/// The kind of an edge (relationship) in the code graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// File defines a symbol (File -> Symbol).
    Defines,
    /// Symbol calls another symbol (Symbol -> Symbol).
    Calls,
    /// File imports from another file/module (File -> File/Module).
    Imports,
    /// A container holds a symbol (Module -> Symbol, Class -> Method).
    Contains,
    /// Symbol uses a type (Function -> Type/Struct/Class).
    UsesType,
    /// Symbol implements a trait/interface (Struct -> Trait).
    Implements,
    /// Class extends another class (Class -> Class).
    Extends,
    /// File exports a symbol (File -> Symbol).
    Exports,
    /// Generic reference between symbols.
    References,
    /// Parameter type relationship (Function -> Type).
    Parameter,
    /// Return type relationship (Function -> Type).
    Returns,
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EdgeKind::Defines => write!(f, "defines"),
            EdgeKind::Calls => write!(f, "calls"),
            EdgeKind::Imports => write!(f, "imports"),
            EdgeKind::Contains => write!(f, "contains"),
            EdgeKind::UsesType => write!(f, "uses_type"),
            EdgeKind::Implements => write!(f, "implements"),
            EdgeKind::Extends => write!(f, "extends"),
            EdgeKind::Exports => write!(f, "exports"),
            EdgeKind::References => write!(f, "references"),
            EdgeKind::Parameter => write!(f, "parameter"),
            EdgeKind::Returns => write!(f, "returns"),
        }
    }
}

/// Data stored in a graph node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeData {
    /// The name of the symbol (e.g., "login", "UserService", "main.rs").
    pub name: String,
    /// What kind of code element this is.
    pub kind: NodeKind,
    /// The file this symbol is defined in.
    pub file_path: PathBuf,
    /// Starting line number (1-indexed).
    pub line_start: usize,
    /// Ending line number (1-indexed).
    pub line_end: usize,
    /// The actual source code snippet.
    pub code_snippet: String,
    /// Lines within this symbol that contain calls to other symbols (1-indexed, absolute).
    /// Used for graph slicing: show only lines where dependencies are used.
    #[serde(default)]
    pub call_lines: Vec<usize>,
    /// Soft-delete flag. Removed nodes are skipped in queries
    /// and cleaned up during compaction.
    #[serde(default)]
    pub removed: bool,
}

impl NodeData {
    pub fn new_file(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Self {
            name,
            kind: NodeKind::File,
            file_path: path,
            line_start: 0,
            line_end: 0,
            code_snippet: String::new(),
            call_lines: Vec::new(),
            removed: false,
        }
    }

    pub fn new_symbol(
        name: String,
        kind: NodeKind,
        file_path: PathBuf,
        line_start: usize,
        line_end: usize,
        code_snippet: String,
    ) -> Self {
        Self {
            name,
            kind,
            file_path,
            line_start,
            line_end,
            code_snippet,
            call_lines: Vec::new(),
            removed: false,
        }
    }
}

/// Data stored on a graph edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeData {
    /// The kind of relationship.
    pub kind: EdgeKind,
}

impl EdgeData {
    pub fn new(kind: EdgeKind) -> Self {
        Self { kind }
    }
}

/// A symbol extracted from parsing a source file.
/// This is an intermediate representation before being added to the graph.
#[derive(Debug, Clone)]
pub struct ExtractedSymbol {
    /// Symbol name.
    pub name: String,
    /// What kind of symbol.
    pub kind: NodeKind,
    /// Line where the symbol starts (1-indexed).
    pub line_start: usize,
    /// Line where the symbol ends (1-indexed).
    pub line_end: usize,
    /// The source code of this symbol.
    pub code_snippet: String,
    /// Parent symbol name (for methods inside classes/impls).
    pub parent: Option<String>,
}

/// An import extracted from a source file.
#[derive(Debug, Clone)]
pub struct ExtractedImport {
    /// The import path or module name.
    pub path: String,
    /// Specific symbols imported (if any).
    pub symbols: Vec<String>,
    /// Line number of the import.
    pub line: usize,
}

/// A function call extracted from a source file.
#[derive(Debug, Clone)]
pub struct ExtractedCall {
    /// The name of the function being called.
    pub callee: String,
    /// The name of the function making the call.
    pub caller: String,
    /// Start line of the call expression (1-indexed).
    pub line: usize,
    /// End line of the call expression (1-indexed). For single-line calls, same as `line`.
    pub line_end: usize,
}

/// All extracted information from a single source file.
#[derive(Debug, Clone)]
pub struct FileExtractions {
    /// Path to the source file.
    pub file_path: PathBuf,
    /// Symbols defined in this file.
    pub symbols: Vec<ExtractedSymbol>,
    /// Import statements.
    pub imports: Vec<ExtractedImport>,
    /// Function/method calls.
    pub calls: Vec<ExtractedCall>,
}

// ─── Graph Search Results ─────────────────────────────────────────────────────

/// Result of a graph-aware search.
/// Contains the matched symbols AND their connections.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphSearchResult {
    /// How the query matched: "file", "symbol", or "none"
    pub match_type: String,
    /// Files that matched (for file-based queries)
    pub matched_files: Vec<PathBuf>,
    /// Symbols found (directly matched + connected via BFS)
    pub symbols: Vec<SymbolInfo>,
    /// Connections between symbols (edges traversed)
    pub connections: Vec<ConnectionInfo>,
    /// True if results were truncated due to limits
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// Information about a symbol in search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: NodeKind,
    pub file: PathBuf,
    pub line: usize,
    pub code: String,
}

/// A connection (edge) between two symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub from: String,
    pub to: String,
    pub relationship: EdgeKind,
}

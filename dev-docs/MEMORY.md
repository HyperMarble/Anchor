# Anchor — Developer Onboarding Guide

> Written for a new developer joining the project. Read this before touching any code.

---

## What Anchor Is

Anchor is **coding infrastructure for AI agents** — a Rust binary that replaces grep/cat/find/read with graph-aware, context-rich tooling.

**The problem it solves**: AI agents waste 60-80% of their token budget doing repeated file reads and searches. `grep login` → read 500-line file → grep again → read another file → repeat. Most of it is noise.

**What Anchor does instead**:
1. Parses the entire codebase once with tree-sitter (14 languages)
2. Builds a directed graph of all symbols and their relationships
3. Serves **graph-sliced context** — only the lines that matter — via MCP or CLI

**One call**: `anchor context login` returns 25 sliced lines + callers + callees + exact line numbers.  
**vs traditional**: grep → read 500 lines → read 3 more files = 2000+ tokens wasted.

---

## Quick Start

```bash
cargo build --release
anchor build              # Index the codebase → .anchor/graph.bin
anchor context login      # Get context for 'login' symbol
anchor search -p '.*Service'   # Find all Services
anchor stats              # File/symbol/edge counts
```

---

## Architecture

```
Parser (tree-sitter)
  ↓ FileExtractions (symbols, calls, imports)
Graph Engine (petgraph)
  ↓ DiGraph<NodeData, EdgeData>
Query Layer (context, search, impact)
  ↓ 
┌─────────────┬─────────────┬──────────────┐
│  MCP Server │  CLI        │  Daemon      │
│  (5 tools)  │  (clap)     │  (unix sock) │
└─────────────┴─────────────┴──────────────┘
```

---

## Module Map

| Module | Path | Purpose |
|--------|------|---------|
| Graph engine | `src/graph/engine.rs` | Core `CodeGraph` struct, build, query |
| Graph types | `src/graph/types.rs` | `NodeData`, `EdgeData`, `NodeKind`, `EdgeKind` |
| Graph query | `src/graph/query.rs` | `search_graph`, scoring, BFS traversal |
| Parser language | `src/parser/language.rs` | `SupportedLanguage` enum, extension → language |
| Extractor | `src/parser/extractor/mod.rs` | `extract_file()` — entry point for parsing |
| Tags | `src/parser/extractor/tags.rs` | tree-sitter query → symbols + calls |
| API queries | `src/parser/queries/api.rs` | Extract HTTP route definitions |
| Context | `src/query/context.rs` | `get_context_for_change()` — high-level query |
| Regex | `src/regex/` | Brzozowski derivative engine (ReDoS-safe) |
| MCP | `src/mcp/tools.rs` | 5 MCP tools: map, search, context, impact, write |
| GraphQL | `src/graphql/` | Internal GQL schema — all queries go through here |
| Lock | `src/lock/manager.rs` | Symbol-level locking for multi-agent safety |
| Write | `src/write.rs` | `replace_range`, `write_ordered`, file mutations |
| Daemon | `src/daemon/server.rs` | Unix socket daemon, `process_request` |
| Watcher | `src/watcher/` | File change watching → incremental graph updates |
| CLI | `src/bin/cli.rs` + `src/cli/` | Entry point, clap commands |
| Init | `src/cli/init.rs` | Agent detection + hook installation |
| Storage | `src/storage/` | `.anchor/` directory management |
| Config | `src/config.rs` | `AnchorConfig` from `.anchor/config.toml` |
| Updater | `src/updater.rs` | Self-update via GitHub releases |

---

## Core Types

### NodeKind
```rust
pub enum NodeKind {
    File, Function, Method, Struct, Class, Interface,
    Enum, Type, Constant, Module, Import, Trait, Impl, Variable,
}
```

### EdgeKind
```rust
pub enum EdgeKind {
    Defines,    // file → function (file contains symbol)
    Calls,      // function → function (call graph)
    Imports,    // file → module (import/use statement)
    Contains,   // class → method
    UsesType, Implements, Extends, Exports, References, Parameter, Returns, ApiCall, EnvRef,
}
```

### NodeData (what's stored per symbol)
```rust
pub struct NodeData {
    pub name: String,
    pub kind: NodeKind,
    pub file_path: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub code_snippet: String,       // Full source code of this symbol
    pub call_lines: Vec<usize>,     // Lines where outgoing calls happen (for slicing)
    pub removed: bool,              // Soft-delete flag
    pub features: Vec<String>,      // Tokenized name parts (for scoring)
}
```

### CodeGraph
```rust
pub struct CodeGraph {
    pub graph: DiGraph<NodeData, EdgeData>,
    file_index: HashMap<PathBuf, NodeIndex>,
    symbol_index: HashMap<String, Vec<NodeIndex>>,        // name → all nodes with that name
    qualified_index: HashMap<(PathBuf, String), NodeIndex>, // (file, name) → unique node
}
```

---

## Key Data Flows

### 1. Build pipeline
```
anchor build
  → walk dirs (gitignore-aware, rayon parallel)
  → for each file: extract_file(path, source)
      → detect language from extension
      → tree-sitter parse → AST
      → run TAGS_QUERY → symbols + calls
      → extract_imports() → imports
      → extract_api_endpoints() → routes
  → collect all FileExtractions
  → graph.build_from_extractions()
      → Phase 1: add all nodes (files, symbols, imports)
      → Phase 2: connect calls by name (symbol_index lookup)
      → populate call_lines per symbol
  → graph.save(".anchor/graph.bin")  ← bincode serialization
```

### 2. MCP context request
```
MCP receives ContextRequest { symbols: ["login"] }
  → load_graph() from .anchor/graph.bin
  → build GraphQL schema with graph as data source
  → execute GQL query: { symbol(name: "login") { code callers { ... } callees { ... } } }
  → format_symbol() → pretty print
  → return to agent
```

### 3. MCP write with impact
```
MCP receives WriteRequest { mode: "range", path, start_line, end_line, new_content }
  → symbols_in_range(path, start..end) → affected symbols
  → get_context_for_change() → callers + suggested edits
  → show agent: "this will break 3 callers..."
  → acquire_symbol_with_wait() → lock symbol + dependents
  → write file
  → rebuild_file(path) → re-extract, update graph
  → release lock
```

---

## Parser — How Extraction Works

Each language has tree-sitter grammar crates (see Cargo.toml). For each file:

1. **Language detection**: `SupportedLanguage::from_path(path)` by extension
2. **Tags query**: Built from `base_tags_query()` + `supplementary_patterns()`
   - Base: grammar's built-in `TAGS_QUERY` (definition/reference patterns)
   - Supplementary: extra patterns we need (scoped calls, type aliases, etc.)
3. **Tags extraction** (`extract_with_tags`):
   - Runs query against AST
   - Capture names: `@definition.function`, `@reference.call`, etc.
   - Maps to `NodeKind`, `EdgeKind`
   - Refines using AST node type (`struct_item` → Struct, not Class)
   - Generates features (tokenized name parts for scoring)

### Language quality tiers
- **Tier 1** (Rust, Python): Comprehensive, dedicated patterns
- **Tier 2** (JS, TS, TSX, Go, Java): Good function/class/call coverage
- **Tier 3** (C#, C++, Ruby, Swift): Generic, some constructs missed
- **Config** (Bash, TOML, YAML, JSON): Custom minimal queries (functions, table headers, keys)

### Config file queries (custom, not from grammar TAGS_QUERY)
- **Bash**: `(function_definition name: (word) @name)`
- **TOML**: `(table (bare_key) @name)` — table headers like `[package]`
- **YAML**: `(block_mapping_pair key: ... @name)` — all mapping keys
- **JSON**: `(document (object (pair key: ... @name)))` — top-level keys only

---

## Regex Engine (Brzozowski Derivatives)

Custom implementation in `src/regex/`. Used for all search pattern matching.

**Why not the `regex` crate?** PCRE can catastrophically backtrack (ReDoS). Brzozowski derivatives are O(n) by construction — mathematically guaranteed, no backtracking ever.

**How it works**:
- `D_c(R)` = derivative of regex `R` with respect to character `c`
- `matches(R, s)` = fold derivatives over `s`, check if result is nullable
- Supports: literals, `.`, `*`, `+`, `?`, `|`, `(`, `)`, `[...]`, `&` (AND), `~` (NOT)

```rust
// Example: search for services
let r = parse(".*Service").unwrap();
let m = Matcher::new(r);
m.is_match("UserService");  // true
m.is_match("AuthService");  // true
m.is_match("Something");    // false
```

---

## Graph Slicing

**The key insight**: Don't send the whole function. Send only what matters.

For a 200-line function, Anchor shows:
- Function signature (line 1)
- Lines where outgoing calls happen (`call_lines`) + ±1 context
- Return statements
- Closing brace
- `...` markers for skipped sections

`call_lines` are populated during graph build: for each symbol, walk its `ExtractedCall` list and record line numbers.

**Known limitation**: Multi-line calls — only first line marked. ±1 context might clip arguments.

---

## Locking (Multi-agent Safety)

When two agents work on the same codebase simultaneously:

```
Agent 1 wants to modify validate()
  → lock_manager.acquire_symbol_with_wait("validate", graph, timeout)
  → Acquires: validate + all its dependents (login, signup, auth_test)
  → Agent 1 writes safely

Agent 2 also wants validate()
  → Returns: Blocked { blocked_by: "Agent1", reason: "..." }
  → Agent 2 must retry or pick different symbol
```

**Limitation**: Locks are in-memory. Don't persist across daemon restarts.

---

## MCP Tools (the 5 tools agents use)

All implemented in `src/mcp/tools.rs`. All use `rmcp` crate (stdio transport).

| Tool | Purpose | Key output |
|------|---------|-----------|
| `context` | Full symbol detail | code + callers + callees |
| `search` | Lightweight lookup | `NAME KIND FILE:LINE` |
| `map` | Codebase overview | modules, entry points, top symbols |
| `impact` | What breaks if you change X | affected callers + suggested fixes |
| `write` | File mutations | range replacement or ordered multi-file writes |

All tools internally: `load_graph()` → build GQL schema → execute GQL → format → return.

**Important**: `context` is exact name lookup via GQL `symbol(name: "...")`.  
`search` converts query to regex `.*query.*` and uses Brzozowski engine.

---

## init — How Anchor Hooks Into AI Agents

`anchor init` detects which AI agents are installed and injects itself:

1. **Detects**: Claude Code, Cursor, Codex, Gemini CLI, Windsurf, Kilo Code, OpenCode, Antigravity
2. **Injects MCP config**: Writes to each agent's settings file (adds `anchor` to MCP servers)
3. **Installs hooks**: Intercepts native tool calls (grep, cat, find, Read, Glob, Grep, etc.) and blocks them, returning: "Use anchor instead: `anchor context <query>`"

This is intentional — adoption is forced, not optional. If agents can still grep, they will.

---

## Dependencies — Why Each Exists

| Crate | Why |
|-------|-----|
| `petgraph` | Directed graph (DiGraph). Stores symbols and relationships |
| `tree-sitter` + 14 grammars | Parse 14 languages into ASTs |
| `async-graphql` | Internal GQL schema — CLI and MCP both execute GQL queries |
| `rmcp` | MCP server (stdio transport) for Claude Code, Cursor, etc. |
| `bincode` | Fast binary serialization for `.anchor/graph.bin` |
| `ignore` | `.gitignore`-aware file traversal |
| `notify` + debouncer | File watching for incremental graph updates |
| `rayon` | Parallel file parsing during `anchor build` |
| `clap` | CLI argument parsing |
| `tokio` | Async runtime for MCP, daemon, watcher |
| `reqwest` + `tar` + `flate2` | Self-updater — downloads binary from GitHub releases |
| `libc` | Unix socket support for daemon |
| `schemars` | JSON schema generation for MCP tool inputs |

### Grammar binary size reference
```
c-sharp:    5.7M   cpp:    3.3M   swift:  3.2M
typescript: 2.8M   ruby:   2.0M   bash:   1.3M
rust:       1.1M   python: 460K   java:   420K
javascript: 372K   go:     228K   yaml:   200K
json:      ~100K   toml:    32K
Total binary: ~34MB release
```

---

## Known Limitations & Gotchas

### Search
- No fuzzy/typo tolerance: `buld_graph` finds nothing
- Results not ranked by relevance — insertion order

### Graph
- Multi-line calls: only first line marked in `call_lines`
- Dynamic dispatch invisible: `obj.method()` — can't resolve to impl
- Re-exports/aliases not resolved: `use foo as bar` may not link `bar()` → `foo`
- Name collision: multiple symbols named `process()` → first match wins

### Persistence
- **Graph format not versioned** — adding new fields to `NodeData` requires full `anchor build`
- No incremental build: full rebuild required on significant changes
- Watcher + `rebuild_file()` exist but disabled for safety (too aggressive)

### Scale
- Untested on 10K+ files — performance/memory unknown
- No monorepo support (no cross-package scoping)

### Locks
- In-memory only — don't survive daemon restart

---

## How to Add a New Language

1. **Cargo.toml**: `tree-sitter-<lang> = "x.x"`
2. **`src/parser/language.rs`**:
   - Add variant to `SupportedLanguage` enum
   - Add extension(s) to `from_path()` match
   - Add `tree_sitter_language()` arm
   - Add `name()` arm
   - Add `same_ecosystem()` arm
3. **`src/parser/extractor/mod.rs`**:
   - Add `base_tags_query()` arm (use grammar's `TAGS_QUERY` or custom const)
   - Add `supplementary_patterns()` arm if needed (or falls through to `_ => ""`)
4. **`src/parser/extractor/tags.rs`**:
   - Add `extract_imports()` arm (return `&[]` if no imports)
5. **`src/parser/queries/api.rs`**:
   - Add arm for `extract_api_endpoints()` (return `vec![]` if no routes)
6. **Verify**: `cargo build` — Rust exhaustive matching catches anything missed

---

## Testing

**105 tests** across all modules:

```bash
cargo test                          # run all
cargo test -- --nocapture           # see println! output
RUST_LOG=debug cargo test           # full tracing output
cargo test graph::tests::           # run only graph tests
```

Test distribution: graph (22), regex (18), parser (8), slicing (4), lock (5), graphql (3), write (3), config (2), integration + misc (rest).

---

## Read Order for New Developer

1. **This file** (MEMORY.md) — you're here
2. **MAIN.md** — high-level overview, motivation, examples
3. `src/graph/types.rs` — understand the data model first
4. `src/graph/engine.rs` — how the graph is built and queried
5. `src/parser/language.rs` + `src/parser/extractor/mod.rs` — how code becomes graph nodes
6. `src/query/context.rs` — what agents actually get back
7. `src/mcp/tools.rs` — the 5 MCP tools (main interface)
8. `src/lock/manager.rs` — multi-agent safety
9. `src/cli/init.rs` — how Anchor hooks into agent environments
10. `src/regex/ast.rs` + `src/regex/derivative.rs` — the regex engine

---

## Roadmap (as of v0.1.6)

**Current**: v0.1.6 — MCP, CLI, daemon, benchmarks, 14-language support, hooks for 8 agents

**Next (v0.1.7)**:
- Graph format versioning (avoid forced rebuilds on schema changes)
- Fuzzy search with ranking
- Multi-line call span detection (fix `call_lines` off-by-one on multi-line calls)

**Later**:
- Incremental graph updates (re-enable watcher safely)
- Monorepo scoping (cross-package graph)
- LSP / VS Code extension
- Dynamic dispatch resolution hints

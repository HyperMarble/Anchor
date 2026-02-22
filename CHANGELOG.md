# Changelog

All notable changes to Anchor are documented here.

## [Unreleased]

### Added
- **Cross-language API boundary detection**: Unified pattern-driven extractor matches route definitions with client calls across languages via `ApiCall` edges
- **Multi-root support**: CLI, MCP server, and daemon all accept multiple `--root` paths to build one unified graph
- **Built-in ignore defaults**: 22 common junk directories (node_modules, target, __pycache__, etc.) are always skipped even without .gitignore
- **CI pipeline**: GitHub Actions workflow with `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- **Tracing**: `RUST_LOG` env-based tracing via tracing-subscriber, outputs to stderr

### Changed
- Incremental updates now clean up stale `ApiCall` edges on file change
- `SearchResult.calls`/`called_by` now include `ApiCall` edges (cross-language connections visible in context)
- Extracted shared helpers in mutation.rs, write.rs, lock/write.rs to eliminate ~120 lines of duplication

### Removed
- Dead code: `scan_stats`/`ScanStats`, `print_banner`, unused `cli::read::read`
- Per-language API extractors (replaced by unified `queries/api.rs`)

## [0.1.5] - 2025-05-xx

### Added
- **Graph-guided writing**: `write_ordered` writes files in dependency order using the code graph
- **Graph slicing**: Only shows lines that matter (call sites, signatures, returns) — reduces context by ~70%
- **Symbol-level locking**: Multi-agent coordination via `LockSymbol`/`UnlockSymbol` daemon commands
- **Incremental graph updates**: `update_file_incremental` diffs old vs new symbols, preserves stable NodeIndex
- **MCP server**: 5 tools — `context`, `search`, `map`, `impact`, `write` — via rmcp over stdio
- **Full code flag**: `--full`/`-F` disables slicing for complete code output
- **Multi-query support**: `context` and `search` accept multiple symbols in one call
- **Semantic features**: Every symbol gets `features: Vec<String>` for feature-based search fallback
- **Coverage indicator**: `[25/88 lines, 3 calls]` prepended to sliced output
- **Benchmark harness**: Python-based SWE-bench importer and task runner
- **Structured XML output**: All CLI commands output XML for AI agent consumption

### Changed
- Consolidated 5 per-language extractors into single `tags.rs` using tree-sitter TAGS queries
- Split `engine.rs` into `query.rs` and `mutation.rs` modules
- Split `mcp.rs` into `tools.rs`, `types.rs`, `format.rs` modules
- Split `lock/mod.rs` into `types.rs`, `manager.rs`, `guard.rs`
- Rewrote README with architecture diagram and MCP integration guide

### Removed
- `plan` command — replaced by multi-symbol context queries

## [0.1.4] - 2025-04-xx

### Added
- `anchor map` command for codebase discovery (modules, entry points, top symbols)
- GraphQL API for search and read operations
- Brzozowski derivatives regex engine for ReDoS-safe pattern matching
- Graph persistence with save/load (bincode serialization)
- ASCII art branding in help and installer
- Query context with dependents and dependencies

### Changed
- Disabled unfinished write/lock/daemon for stable release
- Improved parser, updater, and watcher infrastructure

## [0.1.3] - 2025-03-xx

### Added
- Installation docs for GitHub Pages

## [0.1.2] - 2025-03-xx

### Added
- Self-update from GitHub releases (`anchor update`)
- Unix socket daemon with file watcher for incremental rebuilds
- Auto-start daemon on first command
- Multi-language support: Go, Java, C#, Ruby, C++, Swift
- API endpoint detection for Python (Flask/FastAPI), JS (Express/fetch), Go (Gin/Echo/Chi), Java, C#, Ruby

## [0.1.1] - 2025-02-xx

### Added
- TUI mode for `anchor build` with color palette
- `--no-tui` flag for CI/headless environments
- Star history badge in README

## [0.1.0-alpha] - 2025-01-xx

### Added
- Initial release
- Tree-sitter AST extraction for Rust, Python, JavaScript, TypeScript
- Petgraph-based code graph with soft-delete
- Graph builder with cross-reference resolution
- Basic CLI: `build`, `search`, `read`, `context`

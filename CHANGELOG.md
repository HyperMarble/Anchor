# Changelog

All notable changes to Anchor are documented here.

## [0.1.7] - 2026-05-19

### Added
- **Content-addressed object store** (`.anchor/objects/`): git-style store for parses, slices, and patches keyed by SHA-256 hash. Path index (`paths.json`), symbol index (`symbols.json`), and call index (`calls.json`) built on top. Context projections slice exact line ranges with prefix/suffix hashes for stale-edit detection. Validated on real VS Code corpus: 96.88% avg context reduction, 50/50 lock conflicts rejected, 0 failures
- **lockd — Go lock daemon**: full symbol-level lock daemon over Unix socket (`/tmp/anchor.lock.sock`). Acquire, release, check, list operations. TTL-based lock expiry, stale lock cleanup every 30s, ownership validation, 1MB payload guard, read deadline per request. 22 regression + integration tests
- **lockd wired into write path**: MCP write tool acquires symbol locks via lockd before any file write. Separate tracking for lockd-held vs in-process locks, released via correct paths at all three exit points. Multi-agent write safety is now end-to-end
- **Persistent cross-session cache**: `src/cache.rs` + `.anchor/persistent_cache.json`. Symbol hash stored on disk — unchanged symbols return `CACHED` across sessions without re-sending code
- **Blob extraction** (`src/parser/blob.rs`): universal extractor for non-code files. Markdown → headings as symbols, CSV → rows, TOML/YAML/JSON → whole file, unknown text → brace-based chunking. Files indexed: +16%, skipped: −87%
- **Smart bundling**: `bundle:true` on context tool auto-fetches unseen callees. Stdlib noise filter skips callees with no outgoing calls and universal names (`new`, `from`, `collect`, etc.)
- **BM25 search** (`src/storage/bm25.rs`): TF-IDF ranking with camelCase/snake_case tokenization. Name-token matches get 3x definition boost over path/parent context. Falls back to substring for short queries
- **Adaptive context**: `signature:true` returns only the declaration line. `callers:false` / `callees:false` skip call graph. Agent decides exactly how much it needs per call
- **1MB file size limit in indexer**: files over 1MB skipped — generated fixtures and vendored blobs excluded from agent working context
- **Cross-language pseudocode hypothesis** (`examples/cross_lang_pseudo.rs`): same function in 10 languages collapses to nearly identical pseudocode via tree-sitter AST walking. Validates embedding fine-tuning approach

### Changed
- README rewritten — install, quickstart, MCP setup, supported languages. No over-explanation
- Context tool description updated to reflect adaptive mode
- Caller namespace qualification: callers now show full parent path (e.g. `auth::validate` not `validate`)

## [0.1.6] - 2026-02-xx

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

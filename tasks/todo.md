# Systematic Code Review — Anchor

## Goal
Review every module for bugs, fix them, verify with tests. Module by module.

## Progress

### Completed
- [x] `src/graph/engine.rs` — no bugs
- [x] `src/graph/builder.rs` — no bugs
- [x] `src/graph/mutation.rs` — 3 bugs fixed (call_lines staleness, HashMap collision, resolve_call no file preference)
- [x] `src/query/search.rs` — no bugs (hardcoded limit=5 noted, acceptable)
- [x] `src/query/context.rs` — 2 bugs fixed (find_usages word boundary + multi-match, find_tests broken search)
- [x] `src/query/types.rs` — 1 bug fixed (Signature::parse splits generics naively)
- [x] `src/query/slice.rs` — no bugs

### Completed (continued)
- [x] `src/write.rs` — 2 bugs fixed:
  - Bug D fixed: write_ordered now propagates create_dir_all errors
  - Bug E fixed: topo_sort_ops uses tracing::warn! instead of eprintln!
  - Bug A/B/C: re-analysed, design characteristics not real bugs

### Completed (continued)
- [x] `src/parser/` — 2 bugs fixed:
  - Bug A: generate_features now strips .sh/.toml/.yaml/.yml/.json extensions
  - Bug B: is_scope_kind now includes arrow_function, function_expression, lambda

### Completed (continued)
- [x] `src/mcp/tools.rs` — 3 bugs fixed:
  - Bug A: `map` tool used name-based `dependents()`/`dependencies()` — wrong counts when same name exists in multiple files. Added `callers_for(file, name)` / `callees_for(file, name)` per-node methods to `query.rs` and updated scoped view.
  - Bug B: `eprintln!` in ordered-write and range-write re-index paths — invisible in MCP context. Fixed to `tracing::warn!`.
  - Bug C (pre-session): `load_graph()` cloned entire CodeGraph every call — already fixed to `graph_ref()` / `self.graph.read()` in previous session.

### Pending
- [ ] `src/graphql/` — query.rs, mutation.rs (graphql layer)
- [ ] `src/daemon/` — server, request handling
- [ ] `src/lock/` — LockManager
- [ ] `src/cli/` — CLI commands

## Review Checklist (per module)
- Correctness: does logic match intent?
- Edge cases: empty input, missing files, concurrent access
- Error handling: are errors surfaced or swallowed?
- Data flow: are results computed correctly?
- Tests: do existing tests cover the bugs found?

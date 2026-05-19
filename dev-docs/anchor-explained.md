# How Anchor Works
*Written from the actual source code.*

---

## The Problem

When an AI agent works on a codebase it reads files as text. It has no idea that
`login` calls `validate_token` unless it opens both files and infers the connection.
Every tool call starts from zero. No memory of structure. No understanding of
relationships. Just characters on a page.

Anchor sits between the agent and the codebase. It reads the codebase once,
extracts all meaning from it, builds a graph of everything and how everything
connects, then answers structural questions about that graph instead of making
the agent read files.

---

## The Pipeline: From Code to Graph

### Step 1 — Parsing with Tree-Sitter

Anchor uses tree-sitter to parse every file into an AST. It runs language-specific
tags queries that identify meaningful named things — functions, classes, structs,
methods, types, constants, interfaces, traits, implementations, modules.

For every file it produces a `FileExtractions` struct containing:
- **symbols** — every named definition found
- **calls** — every time one symbol references another, with the exact line number
- **imports** — every import or use statement
- **api_endpoints** — HTTP endpoints detected from route definitions or fetch calls

This runs across 14 languages: Rust, Python, TypeScript, JavaScript, Go, Java,
C#, Ruby, C++, Swift, and more.

### Step 2 — Feature Generation

For each extracted symbol, Anchor generates a list of semantic tokens called
`features`. It splits the symbol's name by snake_case and camelCase boundaries,
adds the kind (function, struct, etc.), adds tokens from the parent scope name,
and adds segments from the file path. Short tokens (2 chars or less) are filtered out.

So `validate_authToken` in `src/auth/handler.rs` inside a class `UserService`
produces features like `["validate", "auth", "token", "function", "user", "service", "auth", "handler"]`.

These features power intent-based search. You can search "find auth handler"
and match a function named `validate_authToken` even without exact name overlap.

### Step 3 — Graph Construction

All symbols become nodes. All calls become directed edges between nodes.
Files become nodes too. The graph is a petgraph `DiGraph<NodeData, EdgeData>`.

**Every node stores:**
- `name` — the symbol's name
- `kind` — what type of thing it is (see NodeKind below)
- `file_path` — which file it lives in
- `line_start`, `line_end` — exact line numbers (1-indexed)
- `code_snippet` — the actual source code of the symbol
- `call_lines` — list of absolute line numbers where this symbol calls others
- `removed` — soft-delete flag (dead nodes are hidden, not physically removed)
- `features` — semantic tokens for intent search

**NodeKind has 14 variants:**
File, Function, Method, Struct, Class, Interface, Enum, Type, Constant,
Module, Variable, Import, Trait, Impl.

**EdgeKind has 13 variants:**
- `Defines` — file defines a symbol
- `Calls` — symbol calls another symbol
- `Imports` — file imports from another
- `Contains` — module or class contains a child symbol
- `Implements`, `Extends` — type hierarchy
- `UsesType`, `Parameter`, `Returns`, `References` — type relationships
- `ApiCall` — cross-language call matched by normalized URL
- `EnvRef` — environment variable reference

### Step 4 — Three-Level Indexing

The graph maintains three indexes for fast lookup:

1. `file_index: HashMap<PathBuf, NodeIndex>` — find a file's node instantly
2. `symbol_index: HashMap<String, Vec<NodeIndex>>` — find all nodes with a given name
   (multiple files can define the same name — this returns all of them)
3. `qualified_index: HashMap<(PathBuf, String), NodeIndex>` — find the exact node
   for a specific (file, name) pair — no ambiguity

The qualified index is what makes per-file operations correct. When Anchor reports
callers of `process` in `auth.rs` specifically, it uses qualified_index to find
that exact node, not all `process` functions across the codebase.

### Step 5 — Call Resolution

After all nodes are added, Anchor resolves calls. For each extracted call
(caller → callee), it looks up the callee by name in symbol_index.
If multiple nodes share the name, it prefers the one in the same file as the
caller. Otherwise it takes the first live match.

It then adds a `Calls` edge and records the line number in the caller node's
`call_lines` field. This line number is the key to slicing (explained below).

### Step 6 — API Boundary Matching

Anchor detects HTTP endpoints on the server side (route definitions like
`router.get("/users/:id", handler)`) and API calls on the client side
(fetch or axios calls like `fetch("/users/123")`). It normalizes both URLs
(`:id` → `{param}`, `/users/123` → `/users/{param}`) and creates `ApiCall`
edges between matching server and client symbols across any language boundary.

---

## Slicing: Fewer Tokens, Same Understanding

The `call_lines` field on each node is why slicing works.

When a function is more than 10 lines and has calls to other symbols, Anchor
slices it instead of returning the full body. It keeps:
- The first line (signature)
- The last line (closing brace)
- Every line that's in `call_lines` (where calls happen)
- One line of context before and after each call line
- Any return statements (`return`, `Ok(`, `Err(`, `raise`, `throw`)
- Everything else becomes `...`

A 200-line function that calls 4 other functions might slice to 25 lines.
The agent sees the skeleton — where it calls things and what it returns —
without reading variable assignments and intermediate logic that don't matter
for structural understanding.

---

## Incremental Indexing

The full graph build takes a few seconds on a large codebase. Anchor doesn't
rebuild it after every write. It rebuilds only the changed file.

The `update_file_incremental` function does a smart diff:

1. Collect old symbols from the file, grouped by name
2. Parse the new file content, extract new symbols grouped by name
3. For each name, compare old count vs new count and content:
   - **Same name, same code** → node is unchanged. Just adjust `call_lines`
     by the line shift (if lines above moved, update stored line numbers)
   - **Same name, different code** → clear old call edges, update the node's
     code and line numbers in place, re-resolve its calls
   - **More new than old** → add new nodes for the extras
   - **More old than new** → soft-delete the extras (set `removed = true`)

Node indexes stay stable across edits. If `login` existed before and still exists
after, it keeps the same NodeIndex. All edges pointing to it remain valid.
Only changed or added symbols get new indexes.

After the diff, Anchor re-resolves calls for any changed or new symbols
and runs `compact()` periodically to physically remove soft-deleted nodes
and reclaim memory.

---

## Search

Anchor has two search modes:

**Exact match** — looks up the name directly in symbol_index. O(1). Instant.

**Fuzzy search** — scores every symbol using `score_symbol()`:
- Name contains the query: score 1-2 depending on whether it's a prefix match
- Feature overlap: score 3-4 if the symbol's semantic tokens overlap with
  the query's tokens

Results are sorted by score, limited to the top N. This is how intent-based
search works — "auth handler" finds `validate_authToken` because the features match
even without the exact name.

---

## Impact Analysis

`get_context_for_change` answers: if I change this symbol, what breaks?

It works in three passes:

**Pass 1 — Find dependents.** Traverse all incoming Calls edges from the target
symbol. These are the callers. For each caller it finds the line number of the call.

**Pass 2 — Signature diff.** If a new signature is provided, Anchor parses both
the old and new signatures (handling generics correctly with depth-aware bracket
tracking), diffs the parameter lists, and identifies added and removed parameters.

**Pass 3 — Generate edits.** For each call site in each caller, it constructs
an `Edit` with the current usage, suggested new usage based on the diff,
and lists of new and removed arguments.

It also looks for tests — functions whose names start with `test_`, `it_`,
`should_`, `check_`, or end with `_test`, `_spec`, that are dependents of
the target or that the target is a dependent of.

---

## Locking

When multiple agents work on the same codebase simultaneously, locking prevents
them from corrupting each other's edits.

A `SymbolKey` is a `(file_path, symbol_name)` pair — the unique identity of a symbol.

When the write tool fires for a range of lines, it first finds all symbols
whose line ranges overlap the edit. Then for each symbol it tries to acquire
a lock via `try_acquire_symbol`:

1. Look up all dependents of the symbol (who calls it)
2. Check if the symbol itself and all its dependents are currently unlocked
3. If any is locked → return `Blocked` with the reason
4. If all are free → acquire all locks atomically

Dependents are locked too because a change to a symbol often forces changes
in its callers. Locking only the symbol while its callers are being edited
simultaneously would cause conflicts.

Two variants exist:
- **try_acquire** — returns immediately with Blocked if contested
- **acquire_with_wait(timeout)** — loops until acquired or timeout expires

Locks release after the write and re-index complete. If the agent crashes,
locks are dropped when the guard is dropped (RAII).

---

## Write Operations

Anchor has two write modes:

**Range mode** (`replace_range`):
1. Find symbols whose line ranges overlap the edit zone
2. Run impact analysis — who calls them, what tests use them
3. Acquire symbol locks
4. Write the new content to disk (with atomic file replacement)
5. Re-index the changed file (`rebuild_file`)
6. Release locks

**Ordered mode** (`write_ordered`):
For multi-file writes, Anchor topologically sorts the operations by dependency
order using Kahn's algorithm. If file A depends on file B, B gets written first.
Cycles get a warning and are appended in original order. Then each file is written
and re-indexed in sorted order.

Both modes use atomic writes — content goes to a `.tmp` file first, synced to disk,
then renamed over the target. No partial writes.

---

## Persistence

The graph is serialized to `.anchor/graph.bin` using bincode. The serialized form
flattens the petgraph structure into:
- A `Vec<NodeData>` (nodes indexed by position)
- A `Vec<(u32, u32, EdgeData)>` (edges as source index, target index, data)

On load, Anchor rebuilds all three indexes from these vectors.

On startup: if `.anchor/graph.bin` exists, load it. Otherwise build fresh and save.
After each write: `rebuild_file` updates the in-memory graph. The binary cache
is not updated on every write — it's a startup cache, not a live journal.

---

## The MCP Interface

Anchor exposes five tools to AI agents via the Model Context Protocol:

**search** — find symbols by name or regex pattern. Returns `NAME KIND FILE:LINE`.
Lightweight. Use before context to narrow down which symbol you want.

**context** — given symbol names, return sliced code + callers + callees + line numbers.
The primary tool. One call replaces reading 3-5 files. Capped at 20KB output.

**map** — codebase overview. Lists modules with symbol counts, identifies entry points
(symbols with no callers but calling others), and top connected symbols by
combined caller+callee count. Optional scope to zoom into a module.

**impact** — given a symbol name and optionally a new signature, return what breaks:
callers affected, suggested edits per call site, related tests.

**write** — range or ordered mode. The only write path. Always includes impact
analysis, locking, and re-indexing.

---

## What Every Anchor Session Generates

Every tool call produces a structured record:
- The graph state at that moment
- The query (what the agent asked)
- The result (what Anchor returned)
- The action the agent took next
- The outcome (did the code compile, did tests pass)

This is a trajectory. Graph state → query → result → action → outcome.

The outcome is binary and ground truth — the code either compiles or it doesn't,
tests either pass or they don't. No human labeling required.

Across thousands of sessions this becomes a dataset of correct agentic behavior
over graph-structured code representations. That dataset is the connection between
what Anchor is today and what a graph-native coding model would be trained on.

---

## What Anchor Is Not

Anchor does not do type checking. It does not resolve generics. It does not know
that `Vec<String>` and `Vec<u8>` are different at the type system level.

Anchor does not verify correctness. A call to a nonexistent function gets recorded
faithfully as a Calls edge pointing nowhere.

Anchor does not search text, comments, or documentation. It finds named code
structure and relationships between named things.

Anchor does not replace the compiler, the LSP, or the test runner. It sits above
the filesystem and below the model — extracting the structural layer that neither
the filesystem nor the model naturally exposes.

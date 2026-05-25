# How Anchor Works

Updated from the current source code after removing stale MCP and old CodeGraph
paths.

## Short Definition

Anchor is a CLI-first agent execution harness for coding agents.

It is not Git. It borrows a few Git-like ideas: content hashes, object-style
storage, indexes, projections, and controlled writes. The goal is different from
Git: Anchor controls how agents read, write, and coordinate inside a working
codebase.

Current Anchor is about three things:

1. Focused context: agents ask for symbols instead of browsing whole files.
2. Checked writes: agents write through Anchor operations instead of blind file
   replacement.
3. Coordination: agents use locks so two sessions do not overwrite the same
   unit of work.

Zev is not part of the current Anchor implementation. Zev is the planned future
read/write representation layer. This file explains Anchor only.

## What Exists Today

Current compiled CLI commands:

```text
anchor build
anchor search <query>
anchor context <symbol>
anchor context <symbol> --bundle
anchor write <path> <content>
anchor edit <path> --action replace --pattern <old> --content <new>
anchor edit <path> --symbol <name> --content <new-symbol-source>
anchor map [scope]
```

Removed from the active code path:

- MCP server code
- old agent init/MCP config wiring
- old watcher/CodeGraph module path
- `.anchor/graph.bin`

Anchor now uses a simple current store built around JSON indexes and
content-addressed objects.

## Current Runtime Loop

```text
project source
  -> anchor build
  -> .anchor/index/*.json
  -> agent calls anchor search/context
  -> agent calls anchor write/edit
  -> Anchor locks the target
  -> Anchor writes the change
  -> Anchor reindexes the changed file
```

The important behavior is that writes refresh the index. The agent should not be
left with stale symbol ranges after a successful Anchor write.

## The .anchor Store

`AnchorStore::init` creates this layout:

```text
.anchor/
  objects/
    parses/
    slices/
    patches/
  index/
    paths.json
    symbols.json
    calls.json
  locks/
    ranges/
  projections/
  writes/
```

The store is Git-like because source states and slices are identified with
hashes:

- `PathEntry.source_hash`: hash of a source file.
- `SymbolEntry.source_hash`: source version the symbol came from.
- `SymbolEntry.slice_hash`: hash of the extracted symbol body.
- `Projection.prefix_hash` and `Projection.suffix_hash`: hash boundaries around
  a projected symbol.
- `object_path(kind, hash)`: maps a SHA256 hash into a content-addressed path.

The current object directories exist, but the main active data path is the JSON
index plus projections.

## Build

`anchor build` walks the repo with `.gitignore` awareness, reads supported files,
extracts symbols and calls, then writes:

```text
.anchor/index/paths.json
.anchor/index/symbols.json
.anchor/index/calls.json
```

Build runs extraction in parallel, then writes the indexes sequentially to avoid
index races.

Unsupported files are skipped. Text/config/blob-like files can still be indexed
through the blob extractor where supported.

## What Gets Indexed

For source files, Anchor extracts:

- symbols: functions, methods, classes, structs, modules, constants, etc.
- imports
- simple caller -> callee relationships
- API-like route/client call information where the parser supports it
- semantic search features from names, parent scopes, kinds, and paths

Supported source languages in the current parser include:

- Rust
- Python
- JavaScript
- TypeScript / TSX
- Go
- Java
- C#
- Ruby
- C++
- Swift

The parser uses tree-sitter for structured source extraction.

## Indexes

### paths.json

Stores file-level state:

```text
path
source_hash
bytes
```

### symbols.json

Stores symbol-level state:

```text
path
source_hash
name
kind
line_start
line_end
slice_hash
features
```

This is the index used by `anchor search`, `anchor context`, and
`anchor edit --symbol`.

### calls.json

Stores lightweight call relationships:

```text
caller_symbol -> [callee_symbol, ...]
```

This is not a full graph database. It is a practical call index used for context
and bundle mode.

## Search

`anchor search` uses the symbol index.

Search has two paths:

1. Basic substring search over symbol names and paths.
2. Hybrid BM25-style search over tokenized symbol features.

Feature tokenization splits names like:

```text
LockManager -> lock, manager
try_acquire_symbol -> try, acquire, symbol
```

This matters for agents because they often know intent before exact names. The
agent can search `lock manager` and still find `LockManager`.

## Context

`anchor context <symbol>` searches for matching symbols, creates a projection for
each result, and prints focused code.

Projection creation verifies that the source file still matches the symbol's
recorded `source_hash`. If the file changed since indexing, projection creation
fails instead of returning stale line ranges.

The context output includes:

- symbol name
- kind
- file
- starting line
- callers from `calls.json`
- callees from `calls.json`
- code slice

## Context Cache

Anchor has a persistent cache under `.anchor/`.

If a symbol's `slice_hash` has already been returned and has not changed, Anchor
can return a cached marker instead of resending the same code again.

That is useful for long-running agent sessions because unchanged code does not
need to consume tokens repeatedly.

The cache is content-based. If the symbol body changes, the `slice_hash` changes
and Anchor sends the new content.

## Bundle Mode

`anchor context <symbol> --bundle` includes selected neighboring callees from the
call index.

The current behavior is:

1. Show the requested symbol context.
2. Read callees from `calls.json`.
3. Add selected project-defined callees that were not already shown.

Bundle mode is a practical context optimization. It reduces repeated
one-symbol-at-a-time lookups when an agent needs immediate local neighborhood
context.

## Write Path

Anchor has two current write commands.

### anchor write

```text
anchor write <path> <content>
```

This creates or overwrites a whole file.

Current behavior:

1. Resolve the path under the selected root.
2. Acquire a file-level lock through lockd if available.
3. Create parent directories if needed.
4. Write the file.
5. Reindex the changed file.
6. Print a machine-readable result.

### anchor edit

Pattern mode:

```text
anchor edit <path> --action replace --pattern <old> --content <new>
anchor edit <path> --action insert --pattern <marker> --content <new>
anchor edit <path> --action delete --pattern <old>
```

Symbol mode:

```text
anchor edit <path> --symbol <name> --content <new-symbol-source>
```

Symbol mode is the more agent-relevant path.

Current symbol edit behavior:

1. Load `.anchor/index/symbols.json`.
2. Find the exact symbol by repo-relative path and name.
3. Create a projection and verify the source hash is still current.
4. Acquire a symbol-level lock through lockd if available.
5. Replace only the symbol's indexed line range.
6. Reindex the changed file.
7. Print a machine-readable result.

This gives agents a direct symbol write path without manually calculating line
ranges.

## Locking

Anchor has two lock implementations:

1. In-process `LockManager`.
2. External `anchor-lockd` Unix socket daemon.

The CLI write path currently uses `anchor-lockd` through the Rust lockd client.

The lock daemon uses keys like:

```text
(symbol, path, owner)
```

For file-level writes, the symbol is:

```text
__file__
```

For symbol-level writes, Anchor hashes the `(path, symbol)` pair into a lockd-safe
symbol name:

```text
sym:<sha256>
```

That avoids lockd validation problems for source symbols with characters that
are not allowed in lock names.

Agent/session ownership comes from:

```text
ANCHOR_AGENT_ID
```

If the variable is absent, Anchor generates a process-local owner ID.

## Current Lock Guarantees

What is proven by tests:

- the same symbol lock blocks a different agent owner
- a file lock blocks a different agent owner
- a different symbol can still be edited while another symbol in the same file is
  locked
- same-owner lock requests are allowed by lockd
- lock owner IDs are normalized into lockd-safe strings

Important limitation:

If `anchor-lockd` is unavailable, the current CLI write path does not hard-fail.
It continues without a cross-process lock. That means lock safety is only a hard
guarantee when lockd is running and reachable.

This is the next important production decision: either keep fail-open for local
developer convenience, or make agent-mode writes fail-closed when lockd is not
available.

## Reindex After Write

After successful `anchor write` or `anchor edit`, Anchor calls
`upsert_symbols_for_path` on the changed file.

That updates:

- path hash
- symbol line ranges
- symbol slice hashes
- symbol names if a write renamed a symbol

This matters because stale context is one of the main ways coding agents make bad
edits. Anchor's write path keeps the index fresh after accepted changes.

## What Anchor Is Right Now

Anchor is currently:

- a working CLI prototype
- a symbol and call indexer
- a focused context provider
- a checked write path
- a symbol-level edit path
- a lockd-aware multi-agent coordination path
- a reindex-after-write loop

Anchor is not currently:

- an MCP server
- a full graph database
- a cloud/team service
- a complete session manager
- a complete write audit UI
- a Zev runtime
- a production-enforced sandbox

## What Still Needs Work

Before calling Anchor a production-grade execution harness, these pieces need to
be finished:

1. Fail-closed lock mode for agent writes when lockd is unavailable.
2. `anchor status` to show session, lock, index, and cache state.
3. First-class write logs exposed through CLI.
4. Stronger multi-agent end-to-end tests with real lockd.
5. Install/adapter wiring for Codex, Claude Code, Cursor, and OpenCode.
6. Clear cloud/team session protocol if the hosted version is built.

The strongest current foundation is the controlled read/write loop:

```text
build -> search/context -> lock -> write/edit -> reindex
```

That loop is what makes Anchor more than a code indexer.

## One-Line Pitch

Anchor is a CLI-first execution harness for coding agents: it gives agents
focused symbol context, checked writes, automatic reindexing, and lock-aware
coordination so multiple agent sessions can work in the same codebase with less
blind reading and fewer write conflicts.

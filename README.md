# Anchor

Anchor is a repo-local execution harness for coding agents working inside real
codebases.

Anchor gives agents a focused context path and a checked write path for project
source. The goal is to make agent work less blind: read the right code, edit
through explicit operations, and coordinate work across agent sessions.

Status: early prototype. Anchor focuses on context, writes, and coordination for
coding agents.

## Install

From crates.io:

```bash
cargo install anchor-sdk
```

From a GitHub release:

```bash
curl -fsSL https://raw.githubusercontent.com/HyperMarble/Anchor/main/docs/install.sh | bash
```

Or build from source:

```bash
git clone https://github.com/HyperMarble/Anchor.git
cd Anchor
cargo build --release
```

## Quick Start

Run Anchor from the root of a project:

```bash
anchor build
anchor search lock manager
anchor context LockManager
anchor context LockManager --bundle
```

Anchor writes its local index to `.anchor/`. The generated store contains path,
symbol, and call indexes used by later `search`, `context`, and `edit` commands.

## Experimental: Prompt Repair

Anchor is also exploring project-aware prompt repair: turning vague or misleading
coding prompts into repo-grounded task briefs before an AI agent starts work.

Example:

```text
fix the agent locks thing so two ais dont mess up the same file lol
```

Anchor can repair that into a task brief that points at the actual Rust/Go lock
paths, warns against invented tools, and suggests the right checks.

This feature is experimental and lives in the benchmark harness today:

```bash
python3 benchmark/prompt_improvement.py --dry-run
```

See [Project-Aware Prompt Repair](docs/prompt-repair.md) for the workflow,
planned CLI, and benchmark strategy.

## Commands

```bash
anchor build
anchor search <query> [query2 ...]
anchor context <symbol> [symbol2 ...]
anchor context <symbol> --bundle
anchor map [scope]
anchor write <path> <content>
anchor edit <path> --action replace --pattern <old> --content <new>
anchor edit <path> --action insert --pattern <marker> --content <new>
anchor edit <path> --action delete --pattern <old>
anchor edit <path> --symbol <name> --content <new-symbol-source>
```

## What It Provides

- focused symbol context instead of whole-file browsing
- hybrid symbol search using tokenized names, paths, kinds, and parents
- checked file writes and symbol-range edits
- automatic reindexing after successful writes
- optional `anchor-lockd` coordination for multi-agent write locks
- tree-sitter based source understanding across common languages
- experimental project-aware prompt repair for safer agent task briefs

## Locking

Anchor's CLI write path talks to `anchor-lockd` when the daemon is available.
The daemon listens on `/tmp/anchor.lock.sock` by default and uses
`ANCHOR_AGENT_ID` to identify agent/session owners.

Start the daemon from source:

```bash
cd lockd
go run .
```

If `anchor-lockd` is unavailable, current CLI writes continue without a
cross-process lock. Treat lock safety as a hard guarantee only when the daemon is
running.

## Supported Languages

Rust, Python, JavaScript, TypeScript, TSX/JSX, Go, Java, C#, Ruby, C++, Swift.

## Current Limits

- Anchor is CLI-first today; the old MCP server path is not part of the active
  implementation.
- It is not a replacement for Git, ripgrep, tree-sitter, formatters, or test
  runners.
- It is not yet a cloud/team service or a complete session manager.
- The strongest current path is local: build an index, retrieve focused context,
  write through checked operations, and reindex changed files.

## License

[Apache-2.0](LICENSE)

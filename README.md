# Anchor

Execution harness and infrastructure layer for AI agents.

AI agents are already capable enough to do serious work. The problem is the
execution environment around them: they search blindly, reread noisy context,
waste tokens, lose track of changes, and quality drops as the session context
fills up.

Anchor sits around the agent and gives it a smaller, sharper execution path. It
turns a task into a compact working set, keeps the important context outside the
model window, routes reads/writes/checks through explicit operations, records
what happened, and coordinates work across agent sessions.

Status: early prototype. The first implementation targets source-code
workspaces, but the product shape is broader: Anchor is the execution layer that
helps agents work with higher efficiency, quality, and safety.

## Install

```bash
cargo install anchor-sdk
```

Or build from source:

```bash
git clone https://github.com/Tharun-10Dragneel/Anchor.git
cd Anchor
cargo build --release
```

## Core Commands

```bash
anchor build                 # index the workspace
anchor task "<intent>"       # get task intake: symbols, slices, related files, likely tests
anchor context <name>        # load focused code context
anchor write <path> <text>   # create or overwrite a file
anchor edit <path> --action replace --pattern <old> --content <new>
anchor edit <path> --symbol <name> --content <replacement>
anchor check -- <command>    # run and record a verification command
anchor status                # summarize execution/provenance signals
anchor trace                 # show recent execution events
anchor receipt               # export machine-readable receipt + quality score
anchor gate --min-score 85   # fail if recorded quality is below threshold
```

## Experimental: Prompt Repair

Anchor is also exploring project-aware prompt repair: turning vague or
misleading coding prompts into repo-grounded task briefs before an AI agent
starts work.

Example:

```text
fix the agent locks thing so two ais dont mess up the same file lol
```

Anchor can repair that into a task brief that points at the actual lock paths,
warns against invented tools, and suggests checks to run.

This feature is experimental and lives in the benchmark harness today:

```bash
python3 benchmark/prompt_improvement.py --dry-run
```

See [Project-Aware Prompt Repair](docs/prompt-repair.md) for the workflow,
planned CLI, and benchmark strategy.

## Strict Mode

By default Anchor degrades gracefully: if the lock daemon is unreachable or a
file has no recorded read, the write proceeds and the gap is recorded as an
event. Set `ANCHOR_STRICT=1` to fail closed instead:

- writes are refused when lockd is unreachable
- existing source files can only be edited by a session that has read them
  through `anchor context`

In both modes, every mutation records a `write.attempt` event *before* the
file is touched — if the event log cannot be written, the write is refused.

## What It Provides

- task-scoped working sets instead of whole-workspace context
- compact code slices, likely files, related files, likely tests, and verification plans
- explicit read, write, edit, run, and check operations
- provenance logs for what the agent read, changed, ran, and verified
- coordination across local or cloud-backed agent sessions
- quality and safety signals from the actual execution trace
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
cross-process lock. Treat lock safety as a hard guarantee only when the daemon
is running.

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

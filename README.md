# Anchor

Agent-neutral execution harness and systems layer for AI coding agents.

AI agents are already capable enough to do serious work. The problem is the
execution environment around them: raw terminal and file access makes them
search blindly, reread noisy context, waste tokens, lose track of changes, edit
against stale state, and let quality drop as the session context fills up.

Anchor sits around any coding agent and gives it a better execution path for
software tasks. It is closer to a kernel/runtime optimizer for agents than to a
new agent: the model still reasons, but Anchor optimizes how the agent searches,
reads, writes, verifies, records, and coordinates code work.

Instead of:

```text
random grep -> huge read -> blind edit -> random test -> hope
```

Anchor pushes the agent toward:

```text
intent -> focused context -> scoped read -> checked write -> targeted verify -> receipt
```

Status: early prototype. The first implementation targets source-code
workspaces, but the product shape is broader: Anchor is the software-task
execution harness that makes coding agents run with higher efficiency, quality,
and safety.

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

`anchor build` also writes `.anchor/product_memory.json` with deterministic
product facts extracted from the README, prompt-repair docs, and manifests.

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

- focused code context instead of whole-workspace or repeated raw reads
- code-unit aware search/read/write paths over files, chunks, symbols, and tests
- checked writes against fresh source state instead of blind file mutation
- verification proof tied to the changed code, not just a final test transcript
- provenance logs for what the agent read, changed, ran, and verified
- coordination across local or cloud-backed agent sessions
- quality signals for broad edits, weak verification, unresolved failures, and AI-slop-style changes
- safety signals for stale context, unrecorded writes, and multi-agent collisions
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

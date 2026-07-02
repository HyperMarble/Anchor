# Anchor

Agent-neutral execution harness and systems layer for AI coding agents.

AI agents are already capable enough to do serious work. The problem is the
execution environment around them: raw terminal and file access makes them
search blindly, reread noisy context, waste tokens, lose track of changes, edit
against stale state, and let quality drop as the session context fills up.

Anchor sits around any coding agent and gives it a controlled execution path for
software tasks. It is closer to a kernel/runtime layer for agents than to a new
agent: the model still reasons and can still use familiar local tools, but Anchor
controls whether the software transaction is scoped, fresh, verified, recorded,
and safe to accept.

Instead of:

```text
random exploration -> broad edit -> random test -> hope
```

Anchor pushes the agent toward:

```text
execution spec -> budgeted work -> scoped patch -> targeted verify -> receipt
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
anchor check -- <command>    # run and record a verification command
anchor status                # summarize execution/provenance signals
anchor trace                 # show recent execution events
anchor receipt               # export machine-readable receipt + quality score
anchor gate --min-score 85   # fail if recorded quality is below threshold
anchor protect on            # optional local protection for source writes
anchor run -- <command>      # run and audit a terminal command
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

## What It Provides

- execution contracts before code work starts
- budgeted evidence, change-surface, verification, and expansion plans
- transaction acceptance for provisional source changes
- verification proof tied to the changed code, not just a final test transcript
- provenance logs for what the agent read, changed, ran, and verified
- coordination across local or cloud-backed agent sessions
- quality signals for broad edits, weak verification, unresolved failures, and AI-slop-style changes
- safety signals for stale context, unrecorded writes, and multi-agent collisions
- tree-sitter based source understanding across common languages
- experimental project-aware prompt repair for safer agent task briefs

## Locking

Anchor's lock daemon is the local coordination primitive for future multi-agent
execution. The daemon listens on `/tmp/anchor.lock.sock` by default and uses
`ANCHOR_AGENT_ID` to identify agent/session owners.

Start the daemon from source:

```bash
cd lockd
go run .
```

Treat lock safety as a hard guarantee only when the daemon is running.

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

# Anchor

Agent execution harness for coding agents working inside real codebases and
workspaces.

Anchor gives agents a focused context path, a checked write path, lock-aware
coordination, and a compact execution record. The goal is to make agent work
less blind: read the right code, edit through explicit operations, verify work,
and leave evidence for humans and later agents.

Status: early prototype. Anchor focuses on context, writes, and coordination for
coding agents.

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
anchor context <name>        # load focused code context
anchor write <path> <text>   # create or overwrite a file
anchor edit <path> --action replace --pattern <old> --content <new>
anchor edit <path> --symbol <name> --content <replacement>
anchor check -- <command>    # run and record a verification command
anchor status                # summarize execution/provenance signals
anchor trace                 # show recent execution events
```

## What It Provides

- focused context for coding agents
- checked write/edit operations
- session and lock-aware agent workflow
- execution event log for reads, writes, locks, and checks
- status signals for context use, edits, conflicts, errors, and verification
- trace output for recent agent execution events
- support for local and cloud-backed agent sessions
- tree-sitter based source understanding across common languages

## Supported Languages

Rust, Python, JavaScript, TypeScript, TSX/JSX, Go, Java, C#, Ruby, C++, Swift.

## License

[Apache-2.0](LICENSE)

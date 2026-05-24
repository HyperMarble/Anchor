# Anchor

Agent execution harness for coding agents working inside real codebases and
workspaces.

Anchor gives agents a focused context path and a checked write path for project
source. The goal is to make agent work less blind: read the right code, edit
through explicit operations, and coordinate work across agent sessions.

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
anchor context <name>        # load focused code context
anchor write <path> <text>   # create or overwrite a file
anchor edit <path> --action replace --pattern <old> --content <new>
```

## What It Provides

- focused context for coding agents
- checked write/edit operations
- session and lock-aware agent workflow
- support for local and cloud-backed agent sessions
- tree-sitter based source understanding across common languages

## Supported Languages

Rust, Python, JavaScript, TypeScript, TSX/JSX, Go, Java, C#, Ruby, C++, Swift.

## License

[Apache-2.0](LICENSE)

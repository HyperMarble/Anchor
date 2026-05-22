# Anchor

Agent execution harness for coding agents working inside real repositories.

Anchor gives agents a focused context path and a checked write path for real
codebases. The goal is to make agent work less blind: read the right code, edit
through explicit operations, and coordinate work across agent sessions.

Status: early prototype. Anchor v1 focuses on context and writes. Anchor v2 is
planned to add Zev, a compact code representation layer for models and agents.

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

## Anchor v2

The next milestone is Zev: a natural compact code representation for models and
agents, with a translation layer back to normal source code such as Python,
Rust, Java, Go, and TypeScript.

## Supported Languages

Rust, Python, JavaScript, TypeScript, TSX/JSX, Go, Java, C#, Ruby, C++, Swift.

## License

[Apache-2.0](LICENSE)

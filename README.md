# Anchor

Controlled CLI runtime for coding agents working inside real repositories.

Anchor gives agents a repo-local way to search code, load focused context, map a
project, and apply writes through explicit commands instead of blind file
browsing.

Status: early prototype. The current public surface is CLI-first. Anchor v2 is
planned to add Zev, a compact code representation layer for agents.

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

## Quick Start

```bash
anchor build                 # index the repository into .anchor/
anchor search <query>        # find symbols
anchor context <name>        # load focused code context
anchor map                   # show a compact codebase map
anchor write <path> <text>   # create or overwrite a file
anchor edit <path> --action replace --pattern <old> --content <new>
```

## What It Provides

- repo-local `.anchor/` store
- symbol search and focused context
- compact codebase maps for agents
- CLI write/edit commands
- tree-sitter based parsing across common languages

## Anchor v2

The next milestone is Zev: a compact, language-neutral code representation that
agents can read and write, with a translation layer back to normal source code
such as Python, Rust, Java, Go, and TypeScript.

## Supported Languages

Rust, Python, JavaScript, TypeScript, TSX/JSX, Go, Java, C#, Ruby, C++, Swift.

## License

[Apache-2.0](LICENSE)

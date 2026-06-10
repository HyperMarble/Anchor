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

## What It Provides

- task-scoped working sets instead of whole-workspace context
- compact code slices, likely files, related files, likely tests, and verification plans
- explicit read, write, edit, run, and check operations
- provenance logs for what the agent read, changed, ran, and verified
- coordination across local or cloud-backed agent sessions
- quality and safety signals from the actual execution trace
- tree-sitter based source understanding across common languages

## Supported Languages

Rust, Python, JavaScript, TypeScript, TSX/JSX, Go, Java, C#, Ruby, C++, Swift.

## License

[Apache-2.0](LICENSE)

# Anchor

Infrastructure for coding AI agents.

> Pre-alpha. Anchor is being rebuilt around a repo-local `.anchor` execution harness.

## What Is Anchor?

Anchor is not a coding agent.

Anchor is the harness layer for agents like Codex, Claude Code, Gemini CLI, Cursor, Continue, and custom MCP clients.

The goal is simple:

```text
same agent + Anchor
  -> less context waste
  -> safer edits
  -> sharper working context
```

Anchor should not replace proven local tools. It should orchestrate them:

```text
rg          -> raw search
tree-sitter -> symbols and ranges
git         -> changed-file truth and diffs
formatters  -> formatting checks
test runners -> verification
```

Anchor adds the deterministic layer those tools do not provide by themselves:

```text
.anchor object store
path and symbol indexes
context projections
edit projections
range locks
write logs
verification records
```

## Current Direction

The old full-repo CodeGraph path is being demoted.

The new MVP direction is a Git-style local store:

```text
.anchor/
  objects/
    parses/
    slices/
    patches/
  index/
    paths.json
    symbols.json
  locks/
    ranges/
  projections/
  writes/
```

Every indexed file is keyed by content hash. If the file changes, Anchor should detect the hash change, reparse only that file, and update the path/symbol indexes.

## MVP Commands

The target command surface is intentionally small:

| Command | Purpose |
|---------|---------|
| `anchor search <query>` | Find candidate files, symbols, and ranges |
| `anchor context <query>` | Return minimal context projections |
| `anchor write ...` | Create, edit, or delete with validation |
| `anchor verify` | Run focused checks and report results |

Current CLI behavior is still transitioning. Some legacy graph-backed commands may exist while the `.anchor` index path is built.

## Why This Exists

Normal agent workflow works, but it is informal:

```text
rg
read files
patch text
run tests
repeat
```

Anchor is useful only if it makes that loop measurably better.

Metrics we care about:

- context bytes read
- files opened
- tool calls
- edit attempts
- patch apply failures
- stale edit rejections
- lock conflicts prevented
- tests passed
- wrong-file edits avoided

The product claim is not "better AI." The product claim is:

```text
Keep your existing agent.
Make its repo interaction cheaper, safer, and sharper.
```

## Current Proof

The validator proof lives in:

```text
tests/validata.rs
tests/validata/git_style_projection.rs
```

Real VS Code corpus probe:

```text
files seen:                 96
symbols tested:             50
avg context reduction:      96.88%
median context reduction:   98.44%
lock conflicts rejected:    50 / 50
verified after edit:        50 / 50
index hash refreshed:       50 / 50
failures:                   0
```

That proves the mechanism, not full agent outcome. The next benchmark is:

```text
Codex alone vs Codex + Anchor
Claude Code alone vs Claude Code + Anchor
Gemini CLI alone vs Gemini CLI + Anchor
```

## Supported Languages

Tree-sitter extraction currently covers:

| Language | Extensions |
|----------|------------|
| Rust | `.rs` |
| Python | `.py`, `.pyw` |
| JavaScript | `.js`, `.mjs`, `.cjs` |
| TypeScript | `.ts`, `.mts`, `.cts` |
| TSX/JSX | `.tsx`, `.jsx` |
| Go | `.go` |
| Java | `.java` |
| C# | `.cs` |
| Ruby | `.rb` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.h` |
| Swift | `.swift` |

## Build From Source

```bash
git clone https://github.com/HyperMarble/Anchor.git
cd Anchor
cargo build --release
```

## MCP

Anchor is intended to expose the harness to agents through MCP.

Example:

```json
{
  "mcpServers": {
    "anchor": {
      "command": "anchor",
      "args": ["mcp"]
    }
  }
}
```

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for the current MVP plan.

## License

[Apache-2.0](LICENSE)

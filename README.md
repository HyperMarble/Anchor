# Anchor

**LSP for AI Agents** 

> **v0.1.0-alpha** — Read-only. Write capabilities coming soon.

---

## Install

```bash
# macOS / Linux
curl -fsSL https://tharun-10dragneel.github.io/Anchor/install.sh | bash
```

Or build from source:
```bash
git clone https://github.com/Tharun-10Dragneel/Anchor.git
cd Anchor
cargo build --release

# Install to system
sudo ./local_install.sh
```

---

## Quick Start

```bash
# Build the code graph for your project (with visual TUI)
anchor build

# Or use CLI-only mode
anchor build --no-tui

# See codebase structure
anchor overview

# Search for a symbol
anchor search "UserService"

# Get full context (code + dependencies + dependents)
anchor context "login"

# See what depends on a symbol
anchor deps "Config"

# Graph stats
anchor stats
```

---

## What is Anchor?

AI agents are good at *reasoning* about code, but bad at:
- Knowing the real structure of a codebase
- Finding symbols without grep spam
- Understanding relationships between files

**Anchor solves this.**

It builds a persistent graph of your codebase that agents can query instantly:
- Where is this symbol defined?
- What calls this function?
- What does this module depend on?

No guessing. No grep. Deterministic answers.

---

## How It Works

```
┌─────────────┐     query      ┌─────────────┐
│   AI Agent  │ ─────────────▶ │   Anchor    │
│  (reasoning)│ ◀───────────── │   (graph)   │
└─────────────┘    context     └─────────────┘
```

1. `anchor build` — Parses your code with tree-sitter, builds a graph
2. Agent queries via CLI or MCP
3. Anchor returns structural facts (not semantic guesses)

---

## Supported Languages

- Rust
- Python
- JavaScript
- TypeScript / TSX

---

## CLI Commands

| Command | Description |
|---------|-------------|
| `anchor build` | Build/rebuild the code graph |
| `anchor overview` | Show codebase structure |
| `anchor search <query>` | Find symbols by name |
| `anchor context <query>` | Get symbol + dependencies + dependents |
| `anchor deps <symbol>` | Show dependency relationships |
| `anchor stats` | Graph statistics |

---

## Roadmap

- [x] Graph engine (petgraph)
- [x] Tree-sitter parsing (Rust, Python, JS, TS)
- [x] CLI tools
- [ ] Graph persistence (save/load)
- [ ] File watching (real-time updates)
- [ ] Write capabilities (safe refactors)
- [ ] MCP server for AI agents

---

## License

Apache-2.0

---

## Star History

## ⭐ Star History

<a href="https://www.star-history.com/#Tharun-10Dragneel/Anchor&Date&legend=bottom-right">
  <picture>
    <source 
      media="(prefers-color-scheme: dark)" 
      srcset="https://api.star-history.com/svg?repos=Tharun-10Dragneel/Anchor&Date&theme=dark&legend=bottom-right" 
    />
    <source 
      media="(prefers-color-scheme: light)" 
      srcset="https://api.star-history.com/svg?repos=Tharun-10Dragneel/Anchor&Date&legend=bottom-right" 
    />
    <img 
      alt="Star History Chart" 
      src="https://api.star-history.com/svg?repos=Tharun-10Dragneel/Anchor&Date&legend=bottom-right" 
    />
  </picture>
</a>

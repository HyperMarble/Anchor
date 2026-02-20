<p align="center">
  <h1 align="center">Anchor</h1>
  <p align="center">Infrastructure for Coding AI agents.</p>
</p>

<p align="center">
  <a href="https://crates.io/crates/anchor-sdk"><img src="https://img.shields.io/crates/v/anchor-sdk.svg" alt="crates.io"></a>
  <a href="https://github.com/Tharun-10Dragneel/Anchor/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
  <a href="https://github.com/Tharun-10Dragneel/Anchor/actions"><img src="https://img.shields.io/github/actions/workflow/status/Tharun-10Dragneel/Anchor/ci.yml?branch=main" alt="CI"></a>
</p>

> **Pre-alpha** — API may change. Install at your own discretion.

---
###What is Anchor?

Anchor is the infrastructure that turns AI coding agents into supercharged powerhouses. It builds a code graph that lets agents search, read, understand, and write code without the clunky file traversal, string reads, and writes that plague current tools. Today's agents treat code like plain text, dumping entire files into the context window and filling it up fast. This leads to bloated prompts, wasted tokens, and poor performance — like putting an auto rickshaw engine in a Ferrari. Anchor solves this by introducing graph slicing for relevant reads and writes, delivering only what's essential.
The result? Agents become hyper-efficient, unlocking these 5 game-changing benefits:

1. Token usage becomes less — Sliced context means 5–10x fewer tokens per query, keeping  costs low and sessions long.

2. Tool calls are less — One graph query gives everything needed, reducing back-and-forth and speeding up workflows.

3. Higher accuracy — The model focuses on reasoning the logic, not parsing noise, leading to fewer hallucinations and better code.

4. Less time — Queries in <200ms, no full rebuilds, and smarter edits mean tasks finish faster.

5. Model actually understands the code — The graph provides structured, relationship-aware context, making agents truly intelligent.

Anchor isn't an agent — it's the backend brain that makes every agent (Claude Code, Cursor, Cline, Aider, custom ones) smarter, faster, and cheaper.

```
anchor context authenticate

authenticate FUNCTION src/auth/login.rs:15
[12/45 lines, 3 calls]
> login_handler    caller
> auth_middleware   caller
< verify_token     callee
< hash_password    callee
< db_lookup        callee
---
15: fn authenticate(user: &str, pass: &str) -> Result<Token> {
20:     let valid = verify_token(&trimmed);
21:     let hashed = hash_password(pass);
22:     let record = db_lookup(&trimmed, &hashed);
45: }
```

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
bash ./local_install.sh
```

## Quick Start

```bash
anchor build                  # Build the code graph (run once per project)
anchor context "login"        # Symbol + callers + callees in one call
anchor search "UserService"   # Find symbols by name
anchor map                    # Codebase overview
```

## Supported Languages

| Language | Extensions | Status |
|----------|-----------|--------|
| Rust | `.rs` | Full |
| Python | `.py` `.pyw` | Full |
| JavaScript | `.js` `.mjs` `.cjs` | Full |
| TypeScript | `.ts` `.mts` `.cts` | Full |
| TSX/JSX | `.tsx` `.jsx` | Full |
| Go | `.go` | Full |
| Java | `.java` | Full |
| C# | `.cs` | Full |
| Ruby | `.rb` | Full |
| C++ | `.cpp` `.cc` `.cxx` `.hpp` `.h` | Full |
| Swift | `.swift` | Full |

## CLI Commands

| Command | Description |
|---------|-------------|
| `anchor build` | Build/rebuild the code graph |
| `anchor context <query>` | Symbol code + callers + callees (primary command) |
| `anchor context --full <query>` | Full unsliced code with line numbers |
| `anchor search <query>` | Find symbols by name (lightweight) |
| `anchor search -p '<regex>'` | Regex pattern search |
| `anchor read <symbol>` | Single symbol with full detail |
| `anchor deps <symbol>` | Dependency relationships |
| `anchor map` | Codebase overview: modules, entry points |
| `anchor stats` | Graph statistics |
| `anchor overview` | Codebase structure summary |
| `anchor mcp` | Start MCP server for AI agent integration |

## MCP Server

Anchor runs as an [MCP](https://modelcontextprotocol.io/) server for AI agents like Claude Code, Cursor, etc.

**Claude Code** — add to your project's `.mcp.json`:

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

MCP tools provided:

| Tool | Description |
|------|-------------|
| `context` | Get symbol code + callers + callees |
| `search` | Find symbols by name or regex |
| `map` | Codebase overview |
| `impact` | What breaks if you change a symbol |
| `write` | Line-range replacement with impact analysis |

## Architecture

```
src/
├── graph/       Code graph engine (petgraph-based)
├── parser/      Tree-sitter AST extraction (per-language)
├── query/       Context building, search, graph slicing
├── mcp/         MCP server for AI agents
├── lock/        Symbol-level locking for parallel writes
├── daemon/      Background daemon with file watching
├── graphql/     Internal GraphQL API
├── regex/       Brzozowski derivative regex engine (ReDoS-safe)
├── cli/         CLI command implementations
└── write/       File write operations
```

## Graph Slicing

Anchor doesn't dump entire files. It **slices** symbol code to show only lines where graph dependencies are called:

- Function signature
- Lines calling other symbols in the graph (with ±1 line context)
- Return statements

A 200-line function becomes ~15 lines showing just the call flow. Use `--full` when you need every line.

## License

[Apache-2.0](LICENSE)

## Star History

<a href="https://www.star-history.com/#Tharun-10Dragneel/Anchor&Date&legend=bottom-right">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Tharun-10Dragneel/Anchor&Date&theme=dark&legend=bottom-right" />
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Tharun-10Dragneel/Anchor&Date&legend=bottom-right" />
    <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Tharun-10Dragneel/Anchor&Date&legend=bottom-right" />
  </picture>
</a>

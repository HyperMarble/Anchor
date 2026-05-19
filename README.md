# Anchor

Agent execution harness for coding AI agents.

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
anchor build          # index the repo
anchor search <query> # find symbols
anchor context <name> # get symbol with callers + callees
anchor write <name>   # safe write with lock + verification
```

## MCP Setup

Add to your MCP config:

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

Tools available to the agent: `anchor_search`, `anchor_context`, `anchor_map`, `anchor_write`, `anchor_impact`.

## Supported Languages

Rust, Python, JavaScript, TypeScript, TSX/JSX, Go, Java, C#, Ruby, C++, Swift.

## License

[Apache-2.0](LICENSE)

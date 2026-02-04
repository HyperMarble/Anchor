## WARNING: THS PROJECT IS PRE APLHA STAGE PLEASE INSTALL THE TOOLS ON YOUR OWN MEANS!!

# Anchor

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

## Supported Languages

- Rust
- Python
- JavaScript
- TypeScript
- Swift 

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

- [x] Graph engine 
- [x] Suported Language (Rust, Python, JS, TS, Swift)
- [x] CLI tools
- [x] Graph persistence (save/load)
- [x] File watching (real-time updates)
- [ ] Write capabilities (safe refactors)
- more to come 

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

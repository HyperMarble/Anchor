#!/usr/bin/env bash
set -euo pipefail

REPO="Tharun-10Dragneel/Anchor"

if [ -z "${INSTALL_DIR:-}" ]; then
  if echo ":$PATH:" | grep -q ":$HOME/.local/bin:"; then
    INSTALL_DIR="$HOME/.local/bin"
  elif echo ":$PATH:" | grep -q ":$HOME/.cargo/bin:"; then
    INSTALL_DIR="$HOME/.cargo/bin"
  else
    INSTALL_DIR="$HOME/.local/bin"
  fi
fi

mkdir -p "$INSTALL_DIR"

echo "Installing Anchor → $INSTALL_DIR"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin)
    case "$ARCH" in
      x86_64) BINARY="anchor-macos-intel" ;;
      arm64)  BINARY="anchor-macos-arm" ;;
      *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  linux)
    case "$ARCH" in
      x86_64) BINARY="anchor-linux-x64" ;;
      *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases" \
  | grep '"tag_name"' | head -1 | cut -d'"' -f4)

[ -z "$LATEST" ] && { echo "Failed to get latest release"; exit 1; }

echo "Version: $LATEST"

URL="https://github.com/$REPO/releases/download/$LATEST/$BINARY.tar.gz"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$URL" | tar -xz -C "$TMP"

if [ ! -f "$TMP/anchor" ]; then
  echo "Downloaded archive does not contain 'anchor' binary"
  exit 1
fi

install_binary () {
  src="$1"
  dest="$2"

  if [ -w "$INSTALL_DIR" ]; then
    cp "$src" "$dest"
    chmod +x "$dest"
  else
    echo "Requesting sudo permission..."
    sudo cp "$src" "$dest"
    sudo chmod +x "$dest"
  fi
}

install_binary "$TMP/anchor" "$INSTALL_DIR/anchor"

echo ""
echo " █████╗ ███╗   ██╗ ██████╗██╗  ██╗ ██████╗ ██████╗"
echo "██╔══██╗████╗  ██║██╔════╝██║  ██║██╔═══██╗██╔══██╗"
echo "███████║██╔██╗ ██║██║     ███████║██║   ██║██████╔╝"
echo "██╔══██║██║╚██╗██║██║     ██╔══██║██║   ██║██╔══██╗"
echo "██║  ██║██║ ╚████║╚██████╗██║  ██║╚██████╔╝██║  ██║"
echo "╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝"
echo "       Agent-Code Interface"
echo ""
echo "Get started:"
echo "  cd your-project"
echo "  anchor build"
echo "  anchor map"
echo ""
echo "Update:    anchor update"
echo "Uninstall: curl -fsSL https://tharun-10dragneel.github.io/Anchor/uninstall.sh | bash"
echo ""

# Setup global agent rules for ALL AI agents
setup_global_agent_rules() {
  local config_dir="$HOME/.config/opencode"
  mkdir -p "$config_dir"
  
  local agents_md="$config_dir/AGENTS.md"
  local begin_marker="<!-- anchor-global-rules:begin -->"
  
  if [ -f "$agents_md" ] && grep -q "$begin_marker" "$agents_md"; then
    echo "Global agent rules already configured."
  else
    local block
    block=$(cat << 'EOF'
<!-- anchor-global-rules:begin -->
# Global Rules

## Anchor Commands for Code Intelligence

When working in codebases with `anchor` installed (check for `.anchor/` directory):

**ALWAYS use anchor commands instead of shell commands:**

- `anchor context <query>` - Get symbol code + callers + callees (USE THIS FIRST)
- `anchor search <query>` - Find symbols by name  
- `anchor context <symbol> --full` - Single symbol full detail
- `anchor map` - Codebase structure overview

**NEVER use these shell commands for code exploration when anchor is available:**
- `grep`, `rg` - use `anchor search` or `anchor context` instead
- `cat`, `head`, `tail` - use `anchor context` instead
- `find`, `fd` - use `anchor search` or `anchor map` instead
- `sed`, `awk` - not needed for code exploration

**Shell commands are still allowed for:**
- Git operations (`git status`, `git diff`, etc.)
- Package managers (`npm`, `cargo`, `pip`, etc.)
- Docker, file system operations (`mkdir`, `rm`, `mv`, `cp`)
- Running tests, builds, etc.

## Anchor Output Format

Anchor returns structured XML output:
```
<results query="Cli" count="1">
<symbol>
<name>Cli</name>
<kind>struct</kind>
<file>/path/to/file.rs</file>
<line>19</line>
<callers>caller1 caller2</callers>
<callees>callee1 callee2</callees>
<code>
  19: pub struct Cli {
  ...
</code>
</symbol>
</results>
```

Use this structured data for understanding code, making edits, and tracking relationships.
EOF
)
    # Compose full block with end marker without clobbering existing file
    block="$block
<!-- anchor-global-rules:end -->"

    if [ -f "$agents_md" ]; then
      if [ -s "$agents_md" ]; then
        printf "\n\n%s\n" "$block" >> "$agents_md"
      else
        printf "%s\n" "$block" > "$agents_md"
      fi
    else
      printf "%s\n" "$block" > "$agents_md"
    fi

    echo "Global agent rules installed to $agents_md (appended safely)"
  fi
}

setup_global_agent_rules

# PATH hint
if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
  if ! echo ":$PATH:" | grep -q ":$INSTALL_DIR:"; then
    echo "Tip: add $INSTALL_DIR to PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  fi
fi

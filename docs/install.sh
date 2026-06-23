#!/usr/bin/env bash
set -euo pipefail

REPO="HyperMarble/Anchor"

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
echo "Uninstall: curl -fsSL https://raw.githubusercontent.com/$REPO/main/docs/uninstall.sh | bash"
echo ""

# Optionally set up OpenCode global agent rules. Disabled by default because
# installing a binary should not silently change agent behavior.
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

Prefer Anchor when you need symbol-level code context:

- `anchor context <query>` - Get symbol code + callers + callees (USE THIS FIRST)
- `anchor search <query>` - Find symbols by name  
- `anchor context <symbol> --full` - Single symbol full detail
- `anchor map` - Codebase structure overview

Use normal shell tools when they are cheaper or clearer:

- `rg`, `grep`, `fd`, `find` for quick literal searches
- `cat`, `sed`, `awk`, `head`, `tail` for small file reads or text processing
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

if [ "${ANCHOR_INSTALL_AGENT_RULES:-0}" = "1" ]; then
  setup_global_agent_rules
else
  echo "Skipped OpenCode agent rules. Set ANCHOR_INSTALL_AGENT_RULES=1 to install them."
fi

# PATH hint
if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
  if ! echo ":$PATH:" | grep -q ":$INSTALL_DIR:"; then
    echo "Tip: add $INSTALL_DIR to PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  fi
fi

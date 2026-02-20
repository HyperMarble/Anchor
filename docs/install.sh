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

# PATH hint
if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
  if ! echo ":$PATH:" | grep -q ":$INSTALL_DIR:"; then
    echo "Tip: add $INSTALL_DIR to PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  fi
fi

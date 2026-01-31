#!/bin/bash
# Anchor installer
# Usage: curl -fsSL https://tharun-10dragneel.github.io/Anchor/install.sh | bash

set -e

REPO="Tharun-10Dragneel/Anchor"
INSTALL_DIR="/usr/local/bin"

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin)
    case "$ARCH" in
      x86_64) BINARY="anchor-macos-intel" ;;
      arm64)  BINARY="anchor-macos-arm" ;;
      *)      echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  linux)
    case "$ARCH" in
      x86_64) BINARY="anchor-linux-x64" ;;
      *)      echo "Unsupported architecture: $ARCH"; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

# Get latest release
LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)

if [ -z "$LATEST" ]; then
  echo "Failed to get latest release"
  exit 1
fi

echo "Installing Anchor $LATEST..."

# Download and extract
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST/$BINARY.tar.gz"
TMP_DIR=$(mktemp -d)

curl -fsSL "$DOWNLOAD_URL" | tar -xz -C "$TMP_DIR"

# Install
sudo mv "$TMP_DIR/anchor" "$INSTALL_DIR/anchor"
sudo mv "$TMP_DIR/anchor-mcp" "$INSTALL_DIR/anchor-mcp"
sudo chmod +x "$INSTALL_DIR/anchor" "$INSTALL_DIR/anchor-mcp"

# Cleanup
rm -rf "$TMP_DIR"

echo "✓ Anchor installed to $INSTALL_DIR"
echo ""
echo "Get started:"
echo "  anchor build     # Build graph for current project"
echo "  anchor overview  # See codebase structure"
echo "  anchor --help    # All commands"

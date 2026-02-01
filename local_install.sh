#!/bin/bash
# Local installer for Anchor (after building from source)
# Usage: sudo ./local_install.sh

set -e

INSTALL_DIR="/usr/local/bin"
BUILD_DIR="target/release"

if [ ! -f "$BUILD_DIR/anchor" ]; then
    echo "Error: anchor binary not found in $BUILD_DIR"
    echo "Please run 'cargo build --release' first"
    exit 1
fi

if [ ! -f "$BUILD_DIR/anchor-mcp" ]; then
    echo "Warning: anchor-mcp binary not found (optional)"
fi

echo "Installing Anchor to $INSTALL_DIR..."

# Install main binary
sudo cp "$BUILD_DIR/anchor" "$INSTALL_DIR/anchor"
sudo chmod +x "$INSTALL_DIR/anchor"

# Install MCP binary if it exists
if [ -f "$BUILD_DIR/anchor-mcp" ]; then
    sudo cp "$BUILD_DIR/anchor-mcp" "$INSTALL_DIR/anchor-mcp"
    sudo chmod +x "$INSTALL_DIR/anchor-mcp"
    echo "✓ anchor-mcp installed"
fi

echo "✓ Anchor installed to $INSTALL_DIR"
echo ""
echo "Get started:"
echo "  anchor build           # Build graph (with visual TUI)"
echo "  anchor build --no-tui  # Build graph (CLI only)"
echo "  anchor overview        # See codebase structure"
echo "  anchor --help          # All commands"
echo ""
echo "🎨 TUI visualization is enabled by default in 'anchor build'!"

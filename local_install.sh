#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
BINARY_PATH="$ROOT_DIR/target/release/anchor"

if [ ! -f "$BINARY_PATH" ]; then
  echo "Binary not found at $BINARY_PATH"
  echo "Run: cargo build --release"
  exit 1
fi

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

if [ -w "$INSTALL_DIR" ]; then
  cp "$BINARY_PATH" "$INSTALL_DIR/anchor"
  chmod +x "$INSTALL_DIR/anchor"
else
  echo "Requesting sudo permission..."
  sudo cp "$BINARY_PATH" "$INSTALL_DIR/anchor"
  sudo chmod +x "$INSTALL_DIR/anchor"
fi

echo "Installed anchor to $INSTALL_DIR/anchor"

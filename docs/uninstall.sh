#!/usr/bin/env bash
set -euo pipefail

if [ -n "${INSTALL_DIR:-}" ]; then
  TARGET_DIRS=("$INSTALL_DIR")
else
  TARGET_DIRS=("$HOME/.local/bin" "$HOME/.cargo/bin" "/usr/local/bin")
fi

remove() {
  path="$1"
  dir=$(dirname "$path")
  if [ -e "$path" ]; then
    if [ -w "$dir" ]; then
      rm -f "$path"
    else
      sudo rm -f "$path"
    fi
  fi
}

for dir in "${TARGET_DIRS[@]}"; do
  remove "$dir/anchor"
done

echo ""
echo "Anchor uninstalled."
echo ""

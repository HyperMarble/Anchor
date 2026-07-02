#!/usr/bin/env bash
set -euo pipefail

ANCHOR_ROOT="${ANCHOR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
WORK_ROOT="${ANCHOR_BENCH_WORK_ROOT:-/Volumes/Hak_SSD/anchor-benchmark-work}"
OUT_DIR="$WORK_ROOT/bin"
OUT_BIN="$OUT_DIR/anchor-linux"
IMAGE="${ANCHOR_LINUX_BUILDER_IMAGE:-rust:bookworm}"
PLATFORM="${ANCHOR_LINUX_PLATFORM:-linux/amd64}"
CONTAINER_NAME="anchor-linux-builder-$$"
PROFILE="${ANCHOR_LINUX_PROFILE:-debug}"

mkdir -p "$OUT_DIR"

docker context use colima >/dev/null

cleanup() {
  docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker create \
  --name "$CONTAINER_NAME" \
  --platform "$PLATFORM" \
  "$IMAGE" \
  sleep infinity >/dev/null

docker start "$CONTAINER_NAME" >/dev/null
docker exec "$CONTAINER_NAME" mkdir -p /work

tar -C "$ANCHOR_ROOT" \
  --exclude .git \
  --exclude target \
  --exclude benchmark/results \
  -cf - . | docker exec -i "$CONTAINER_NAME" tar -C /work -xf -

if [[ "$PROFILE" == "release" ]]; then
  BUILD_CMD='cargo build --release --bin anchor -j1'
  BUILT_BIN='/work/target/release/anchor'
else
  BUILD_CMD='cargo build --bin anchor -j1'
  BUILT_BIN='/work/target/debug/anchor'
fi

docker exec -w /work "$CONTAINER_NAME" bash -lc \
  "export PATH=/usr/local/cargo/bin:\$PATH && cargo --version && $BUILD_CMD"

docker cp "$CONTAINER_NAME:$BUILT_BIN" "$OUT_BIN"

chmod +x "$OUT_BIN"
echo "$OUT_BIN"

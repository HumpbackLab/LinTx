#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)

BOARD_HOST="${BOARD_HOST:-root@10.85.35.1}"
BOARD_PASSWORD="${BOARD_PASSWORD:-root}"
BOARD_DIR="${BOARD_DIR:-/root/lintx}"
TARGET_BIN="${TARGET_BIN:-$REPO_ROOT/target/riscv64gc-unknown-linux-musl/release/LinTx}"

SSH_CMD="sshpass -p $BOARD_PASSWORD ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null $BOARD_HOST"

if [ ! -x "$TARGET_BIN" ]; then
    echo "missing binary: $TARGET_BIN" >&2
    echo "build first: cross build --target riscv64gc-unknown-linux-musl --release --features lvgl_ui" >&2
    exit 1
fi

cd "$REPO_ROOT"

$SSH_CMD "mkdir -p '$BOARD_DIR'"

tar \
    --exclude='./.git' \
    --exclude='./target' \
    --exclude='*/target' \
    --exclude='./.DS_Store' \
    -cf - . \
    | sshpass -p "$BOARD_PASSWORD" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null "$BOARD_HOST" \
        "mkdir -p '$BOARD_DIR' && tar -xf - -C '$BOARD_DIR'"

sshpass -p "$BOARD_PASSWORD" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null "$BOARD_HOST" \
    "cat > '$BOARD_DIR/LinTx'" < "$TARGET_BIN"
sshpass -p "$BOARD_PASSWORD" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null "$BOARD_HOST" \
    "chmod +x '$BOARD_DIR/LinTx' '$BOARD_DIR/start' '$BOARD_DIR/start.sh' && find '$BOARD_DIR/scripts/board' -type f -name '*.sh' -exec chmod +x {} ';' && find '$BOARD_DIR/scripts/board' -type f -name '*.py' -exec chmod +x {} ';'"

echo "deployed to $BOARD_HOST:$BOARD_DIR"

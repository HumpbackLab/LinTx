#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/../lib/board_common.sh"

DEV_NAME="${1:-/dev/ttyS0}"
BAUDRATE="${2:-115200}"

stop_lintx
start_server

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- stm32_serial "$DEV_NAME" --baudrate "$BAUDRATE" \
    >"$LOG_DIR/input_stm32.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- rc_button_input \
    >"$LOG_DIR/rc_button_input.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mixer \
    >"$LOG_DIR/mixer.log" 2>&1

start_ui_fb
sleep 2
show_status

cat <<EOF
STM32 input test started.
UART: $DEV_NAME
Baudrate: $BAUDRATE

Open Control page and verify:
- Source = STM32 Serial
- Status = Running
- Channel values follow stick movement
- Mixer output follows stick movement
- GUI five-way key follows Up/Down/Left/Right/Open/Back mapping
EOF

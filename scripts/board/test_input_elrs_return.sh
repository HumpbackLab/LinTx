#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/board_common.sh"

STM32_DEV_NAME="${1:-/dev/ttyS0}"
STM32_BAUDRATE="${2:-115200}"
RF_DEV_NAME="${3:-/dev/ttyS2}"
RF_BAUDRATE="${4:-115200}"

stop_lintx
start_server

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- stm32_serial "$STM32_DEV_NAME" --baudrate "$STM32_BAUDRATE" \
    >"$LOG_DIR/input_stm32.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- rc_button_input \
    >"$LOG_DIR/rc_button_input.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mixer \
    >"$LOG_DIR/mixer.log" 2>&1

# Keep legacy alias support in case board image is not updated yet.
if ! LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- rf_link_service "$RF_DEV_NAME" --baudrate "$RF_BAUDRATE" \
    >"$LOG_DIR/rf_link_service.log" 2>&1; then
    LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- elrs_tx "$RF_DEV_NAME" --baudrate "$RF_BAUDRATE" \
        >"$LOG_DIR/elrs_tx.log" 2>&1
fi

start_ui_fb
sleep 2
show_status

cat <<EOF2
ELRS return-path input test started.
STM32 UART: $STM32_DEV_NAME @ $STM32_BAUDRATE
RF UART:    $RF_DEV_NAME @ $RF_BAUDRATE

Open Control/System page and verify:
- Input Source remains STM32 Serial (or your configured main input source)
- ELRS feedback line updates (connected/signal/battery when available)
- Signal/Aircraft Battery can change when telemetry is present
- Mixer output and control link keep running (no freeze while parsing telemetry)
EOF2

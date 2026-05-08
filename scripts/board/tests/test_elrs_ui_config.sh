#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/../lib/board_common.sh"

RF_DEV_NAME="${1:-/dev/ttyS2}"
RF_BAUDRATE="${2:-115200}"
INPUT_MODE="${3:-mock}" # mock | stm32
STM32_DEV_NAME="${4:-/dev/ttyS0}"
STM32_BAUDRATE="${5:-115200}"

stop_lintx
start_server

case "$INPUT_MODE" in
    mock)
        LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mock_joystick \
            >"$LOG_DIR/input_mock.log" 2>&1
        ;;
    stm32)
        LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- stm32_serial "$STM32_DEV_NAME" --baudrate "$STM32_BAUDRATE" \
            >"$LOG_DIR/input_stm32.log" 2>&1
        ;;
    *)
        echo "Invalid INPUT_MODE: $INPUT_MODE (expected: mock|stm32)" >&2
        exit 2
        ;;
esac

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- rc_button_input \
    >"$LOG_DIR/rc_button_input.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mixer \
    >"$LOG_DIR/mixer.log" 2>&1

if ! LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- rf_link_service "$RF_DEV_NAME" --baudrate "$RF_BAUDRATE" \
    >"$LOG_DIR/rf_link_service.log" 2>&1; then
    LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- elrs_tx "$RF_DEV_NAME" --baudrate "$RF_BAUDRATE" \
        >"$LOG_DIR/elrs_tx.log" 2>&1
fi

start_ui_fb
sleep 2
show_status

cat <<EOF
ELRS UI config test started.
RF UART: $RF_DEV_NAME @ $RF_BAUDRATE
Input mode: $INPUT_MODE

Open launcher page "ELRS" (Scripts app), then test:
1) Manual WiFi: Left/Right or Enter toggle ON/OFF
2) Bind Mode: Left/Right or Enter toggle ACTIVE/IDLE
3) TX Power: Left/Right adjust 10/25/100/250/500/1000mW
4) Bind Phrase: Enter to edit; Up/Down change char; Left/Right move cursor; Enter save; Back cancel

Tips:
- Refresh key: ']' (PageNext)
- Config persists in radio.toml -> [elrs]
- Default RF UART is /dev/ttyS2
EOF

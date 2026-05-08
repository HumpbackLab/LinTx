#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
APP_DIR="${APP_DIR:-$SCRIPT_DIR}"
. "$SCRIPT_DIR/scripts/board/lib/board_common.sh"

RF_DEV_NAME="${1:-/dev/ttyS2}"
RF_BAUDRATE="${2:-115200}"
INPUT_MODE="${3:-stm32}" # stm32 | mock
STM32_DEV_NAME="${4:-/dev/ttyS0}"
STM32_BAUDRATE="${5:-115200}"

stop_lintx

USB_GAMEPAD_SETUP="$SCRIPT_DIR/scripts/board/usb_gamepad/setup_hid_gamepad.sh"
if [ -x "$USB_GAMEPAD_SETUP" ] || [ -f "$USB_GAMEPAD_SETUP" ]; then
    if [ ! -c /dev/hidg0 ]; then
        sh "$USB_GAMEPAD_SETUP" >"$LOG_DIR/hid_setup_start.log" 2>&1 || true
        sleep 3
    fi
fi

start_server

case "$INPUT_MODE" in
    stm32)
        LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- stm32_serial "$STM32_DEV_NAME" --baudrate "$STM32_BAUDRATE" \
            >"$LOG_DIR/input_stm32.log" 2>&1
        ;;
    mock)
        LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mock_joystick \
            >"$LOG_DIR/input_mock.log" 2>&1
        ;;
    *)
        echo "Invalid INPUT_MODE: $INPUT_MODE (expected: stm32|mock)" >&2
        exit 2
        ;;
esac

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- rc_button_input \
    >"$LOG_DIR/rc_button_input.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mixer \
    >"$LOG_DIR/mixer.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- usb_gamepad_control \
    >"$LOG_DIR/usb_gamepad_control.log" 2>&1

if ! LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- rf_link_service "$RF_DEV_NAME" --baudrate "$RF_BAUDRATE" \
    >"$LOG_DIR/rf_link_service.log" 2>&1; then
    LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- elrs_tx "$RF_DEV_NAME" --baudrate "$RF_BAUDRATE" \
        >"$LOG_DIR/elrs_tx.log" 2>&1
fi

start_ui_fb
sleep 2
show_status

cat <<EOF
LinTx full startup complete.
RF UART:    $RF_DEV_NAME @ $RF_BAUDRATE
Input mode: $INPUT_MODE
STM32 UART: $STM32_DEV_NAME @ $STM32_BAUDRATE

Override example:
  ./start
  ./start /dev/ttyS2 115200 mock
  ./start /dev/ttyS2 115200 stm32 /dev/ttyS0 115200
EOF

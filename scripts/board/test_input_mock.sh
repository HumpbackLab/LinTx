#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/board_common.sh"

stop_lintx
start_server

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mock_joystick \
    >"$LOG_DIR/input_mock.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- rc_button_input \
    >"$LOG_DIR/rc_button_input.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mixer \
    >"$LOG_DIR/mixer.log" 2>&1

start_ui_fb
sleep 2
show_status

cat <<'EOF'
Input mock test started.
Open Control page and verify:
- Source = Mock
- Status = Running
- Channel values are changing or non-zero
- Mixer output is not stuck at the UI default
EOF

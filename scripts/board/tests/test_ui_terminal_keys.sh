#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/../lib/board_common.sh"

stop_lintx
start_server

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mock_joystick \
    >"$LOG_DIR/input_mock.log" 2>&1

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- mixer \
    >"$LOG_DIR/mixer.log" 2>&1

start_ui_fb
sleep 1
show_status

cat <<'EOF_INFO'
Terminal UI key injector started.

Use keys in this SSH session:
- Arrow keys or WASD: move
- Enter: open/select
- Esc or b: back
- [ / ]: switch launcher page
- q: quit injector (and send UI quit)

Press q to stop.
EOF_INFO

sh "$SCRIPT_DIR/../tools/ui_key_input.sh"

#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/../lib/board_common.sh"

cat <<'EOF_INFO'
Attach terminal key injector.

Use keys in this SSH session:
- Arrow keys or WASD: move
- Enter: open/select
- Esc or b: back
- [ / ]: switch launcher page
- q: quit injector (and send UI quit)

Press q to stop.
EOF_INFO

TTY_DEV="/dev/tty"
UI_FIFO_PATH="${UI_FIFO_PATH:-/tmp/lintx-ui-input.fifo}"
if [ ! -r "$TTY_DEV" ]; then
    echo "Cannot access $TTY_DEV. Please run in an interactive SSH terminal." >&2
    exit 1
fi

LINTX_SOCKET_PATH="$SOCKET_PATH" "$BIN" --detach -- ui_input_fifo --pipe-path "$UI_FIFO_PATH" \
    >"$LOG_DIR/ui_input_fifo.log" 2>&1 || true

for _ in 1 2 3 4 5 6 7 8 9 10; do
    [ -p "$UI_FIFO_PATH" ] && break
    sleep 0.1
done
if [ ! -p "$UI_FIFO_PATH" ]; then
    echo "FIFO not ready: $UI_FIFO_PATH" >&2
    exit 1
fi

ORIG_STTY=$(stty -g <"$TTY_DEV")
restore_tty() {
    stty "$ORIG_STTY" <"$TTY_DEV" || true
}
trap restore_tty EXIT INT TERM
stty raw -echo <"$TTY_DEV"

exec 3>"$UI_FIFO_PATH"

read_char() {
    dd if="$TTY_DEV" bs=1 count=1 2>/dev/null || true
}

read_char_timeout() {
    stty -echo -icanon min 0 time "$1" <"$TTY_DEV"
    c=$(dd if="$TTY_DEV" bs=1 count=1 2>/dev/null || true)
    stty raw -echo <"$TTY_DEV"
    printf '%s' "$c"
}

send_evt() {
    printf '%s\n' "$1" >&3
}

ESC=$(printf '\033')
CR=$(printf '\r')
LF=$(printf '\n')

while :; do
    ch=$(read_char)
    [ -n "$ch" ] || continue
    case "$ch" in
        q|Q) send_evt quit; break ;;
        "$CR"|"$LF") send_evt open ;;
        '[') send_evt page-prev ;;
        ']') send_evt page-next ;;
        w|W|k|K) send_evt up ;;
        s|S|j|J) send_evt down ;;
        a|A|h|H) send_evt left ;;
        d|D|l|L) send_evt right ;;
        b|B) send_evt back ;;
        "$ESC")
            second=$(read_char_timeout 1)
            if [ -z "$second" ]; then
                send_evt back
                continue
            fi
            if [ "$second" = "[" ] || [ "$second" = "O" ]; then
                third=$(read_char_timeout 1)
                case "$third" in
                    A) send_evt up ;;
                    B) send_evt down ;;
                    C) send_evt right ;;
                    D) send_evt left ;;
                esac
            fi
            ;;
    esac
done

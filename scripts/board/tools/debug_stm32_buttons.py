#!/usr/bin/env python3
import argparse
import fcntl
import os
import signal
import sys
import termios
import time

LOCK_PATH = "/tmp/lintx_stm32_buttons.lock"
SYNC = 0x5A
TYPE_JOYSTICK = 0x01
LEGACY_PAYLOAD_LEN = 12
EXTENDED_PAYLOAD_LEN = 14
MAX_FRAME_LEN = 60

GUI_BITS = {
    10: "gui_up",
    11: "gui_down",
    12: "gui_left",
    13: "gui_right",
    14: "gui_press",
}

BUTTON_BITS = {
    0: "key1_up",
    1: "key1_down",
    2: "key1_left",
    3: "key1_right",
    4: "key1_press",
    5: "key2_up",
    6: "key2_down",
    7: "key2_left",
    8: "key2_right",
    9: "key2_press",
    **GUI_BITS,
    15: "reserved",
}

BAUDS = {
    9600: termios.B9600,
    19200: termios.B19200,
    38400: termios.B38400,
    57600: termios.B57600,
    115200: termios.B115200,
    230400: getattr(termios, "B230400", termios.B115200),
    460800: getattr(termios, "B460800", termios.B115200),
    921600: getattr(termios, "B921600", termios.B115200),
}


def crc8_dvb_s2(data):
    crc = 0
    for byte in data:
        crc ^= byte
        for _ in range(8):
            if crc & 0x80:
                crc = ((crc << 1) ^ 0xD5) & 0xFF
            else:
                crc = (crc << 1) & 0xFF
    return crc


def open_serial(path, baudrate, configure=True):
    if not configure:
        return os.open(path, os.O_RDONLY | os.O_NOCTTY)
    if baudrate not in BAUDS:
        raise SystemExit(f"unsupported baudrate {baudrate}; add it to BAUDS in this script")
    fd = os.open(path, os.O_RDONLY | os.O_NOCTTY)
    attrs = termios.tcgetattr(fd)
    attrs[0] = 0
    attrs[1] = 0
    attrs[2] = termios.CLOCAL | termios.CREAD | termios.CS8
    attrs[3] = 0
    attrs[4] = BAUDS[baudrate]
    attrs[5] = BAUDS[baudrate]
    attrs[6][termios.VMIN] = 1
    attrs[6][termios.VTIME] = 1
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    termios.tcflush(fd, termios.TCIFLUSH)
    return fd


def acquire_lock(kill_existing=False):
    lock_fd = os.open(LOCK_PATH, os.O_RDWR | os.O_CREAT, 0o644)
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        os.lseek(lock_fd, 0, os.SEEK_SET)
        existing = os.read(lock_fd, 64).decode(errors="ignore").strip()
        existing_pid = int(existing) if existing.isdigit() else None
        if not kill_existing or existing_pid is None:
            os.close(lock_fd)
            raise SystemExit(
                f"another debug_stm32_buttons.py is running"
                f"{f' as pid {existing_pid}' if existing_pid else ''}; "
                f"use --kill-existing to stop it"
            )

        try:
            os.kill(existing_pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        deadline = time.monotonic() + 2.0
        while time.monotonic() < deadline:
            try:
                fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                time.sleep(0.05)
        else:
            try:
                os.kill(existing_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            fcntl.flock(lock_fd, fcntl.LOCK_EX)

    os.ftruncate(lock_fd, 0)
    os.write(lock_fd, str(os.getpid()).encode())
    os.fsync(lock_fd)
    return lock_fd


def read_u16_le(payload, offset):
    return payload[offset] | (payload[offset + 1] << 8)


def parse_payload(payload):
    if len(payload) not in (LEGACY_PAYLOAD_LEN, EXTENDED_PAYLOAD_LEN):
        return None
    if payload[0] != TYPE_JOYSTICK:
        return None

    axes = [read_u16_le(payload, 1 + i * 2) for i in range(4)]
    switch_byte = payload[9]
    shoulder_byte = payload[10]
    buttons = read_u16_le(payload, 11) if len(payload) == EXTENDED_PAYLOAD_LEN else 0
    switch_3pos = [
        switch_byte & 0b11,
        (switch_byte >> 2) & 0b11,
        (switch_byte >> 4) & 0b11,
        (switch_byte >> 6) & 0b11,
    ]
    switch_2pos = [bool(shoulder_byte & 0x01), bool(shoulder_byte & 0x02)]
    return {
        "axes": axes,
        "switch_3pos": switch_3pos,
        "switch_2pos": switch_2pos,
        "buttons": buttons,
        "extended": len(payload) == EXTENDED_PAYLOAD_LEN,
    }


def button_names(buttons):
    names = [name for bit, name in BUTTON_BITS.items() if buttons & (1 << bit)]
    return names if names else ["none"]


def gui_hint(buttons):
    if buttons & (1 << 12) and buttons & (1 << 14):
        return "GUI: Back (left+press)"
    if buttons & (1 << 14):
        return "GUI: Open/Back(long press)"
    active = [name for bit, name in GUI_BITS.items() if buttons & (1 << bit)]
    return "GUI: " + ",".join(active) if active else "GUI: idle"


def format_frame(seq, parsed, crc_ok, raw=False, payload=None, framed=True):
    buttons = parsed["buttons"]
    parts = [
        f"#{seq:06d}",
        "5a" if framed else "bare",
        "ext" if parsed["extended"] else "legacy",
        "crc=ok" if crc_ok else "crc=BAD",
        f"axes={parsed['axes']}",
        f"3pos={parsed['switch_3pos']}",
        f"2pos={parsed['switch_2pos']}",
        f"buttons=0x{buttons:04X}",
        "bits=" + ",".join(button_names(buttons)),
        gui_hint(buttons),
    ]
    if raw and payload is not None:
        parts.append("payload=" + payload.hex(" "))
    return " | ".join(parts)


def pop_candidate_frame(buf):
    if not buf:
        return None

    if buf[0] == SYNC:
        if len(buf) < 2:
            return None
        payload_len = buf[1]
        if payload_len < 2 or payload_len > MAX_FRAME_LEN:
            del buf[0]
            return False
        frame_len = 2 + payload_len
        if len(buf) < frame_len:
            return None
        payload = bytes(buf[2:frame_len])
        del buf[:frame_len]
        return True, payload

    if buf[0] in (LEGACY_PAYLOAD_LEN, EXTENDED_PAYLOAD_LEN):
        payload_len = buf[0]
        frame_len = 1 + payload_len
        if len(buf) < frame_len:
            return None
        payload = bytes(buf[1:frame_len])
        del buf[:frame_len]
        return False, payload

    del buf[0]
    return False


def parse_stream(fd, args):
    buf = bytearray()
    seq = 0
    last_printed = None
    last_report = time.monotonic()
    live = args.live
    live_interval = 1.0 / args.live_hz if args.live_hz > 0 else args.heartbeat

    while True:
        chunk = os.read(fd, 256)
        if not chunk:
            continue
        if args.dump_bytes:
            print("rx=" + chunk.hex(" "), flush=True)
            if args.no_parse:
                continue
        buf.extend(chunk)

        while True:
            candidate = pop_candidate_frame(buf)
            if candidate is None:
                break
            if candidate is False:
                continue
            framed, payload = candidate

            crc_ok = crc8_dvb_s2(payload[:-1]) == payload[-1]
            if not crc_ok and not args.show_bad_crc:
                continue
            parsed = parse_payload(payload)
            if parsed is None:
                if args.raw:
                    print(f"unknown payload len={len(payload)} data={payload.hex(' ')}")
                continue

            seq += 1
            key = (
                tuple(parsed["axes"]),
                tuple(parsed["switch_3pos"]),
                tuple(parsed["switch_2pos"]),
                parsed["buttons"],
                crc_ok,
            )
            now = time.monotonic()
            if live:
                should_print = (
                    args.all or key != last_printed or now - last_report >= live_interval
                )
            else:
                should_print = args.all or key != last_printed or now - last_report >= args.heartbeat
            if should_print:
                line = format_frame(seq, parsed, crc_ok, args.raw, payload, framed)
                if live:
                    print("\r\033[K" + line, end="", flush=True)
                else:
                    print(line, flush=True)
                last_printed = key
                last_report = now
            if args.once:
                if live:
                    print()
                return


def main():
    parser = argparse.ArgumentParser(
        description="Decode LinTx STM32 joystick/switch/buttons frames from UART."
    )
    parser.add_argument("dev", nargs="?", default="/dev/ttyS0", help="serial device")
    parser.add_argument("--baudrate", "-b", type=int, default=115200)
    parser.add_argument("--all", action="store_true", help="print every decoded frame")
    parser.add_argument("--raw", action="store_true", help="include payload hex")
    parser.add_argument("--once", action="store_true", help="exit after first decoded frame")
    parser.add_argument("--show-bad-crc", action="store_true", help="print frames with bad CRC")
    parser.add_argument("--dump-bytes", action="store_true", help="print raw UART bytes before parsing")
    parser.add_argument("--no-parse", action="store_true", help="only dump bytes, do not parse frames")
    parser.add_argument("--no-configure", action="store_true", help="do not change serial termios; read like cat")
    parser.add_argument("--log", action="store_true", help="deprecated; line-log mode is now the default")
    parser.add_argument("--live", action="store_true", help="live-update one terminal line instead of line logs")
    parser.add_argument("--live-hz", type=float, default=10.0, help="max unchanged live refresh rate")
    parser.add_argument("--heartbeat", type=float, default=2.0, help="seconds between unchanged reports")
    parser.add_argument("--kill-existing", action="store_true", help="stop an existing debugger instance before opening serial")
    args = parser.parse_args()

    print(
        f"Opening {args.dev} @ {args.baudrate}. Expect legacy len=12 or extended len=14 frames.",
        file=sys.stderr,
    )
    print("Press Ctrl-C to stop.", file=sys.stderr)
    lock_fd = acquire_lock(args.kill_existing)
    fd = open_serial(args.dev, args.baudrate, configure=not args.no_configure)
    try:
        parse_stream(fd, args)
    except KeyboardInterrupt:
        if args.live:
            print()
    finally:
        os.close(fd)
        os.close(lock_fd)


if __name__ == "__main__":
    main()

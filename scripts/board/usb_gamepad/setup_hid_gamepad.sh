#!/bin/sh
# USB HID Gamepad 统一设置脚本
# 作用：在现有 g0 gadget 上添加/更新 HID 手柄功能
# 
# 使用方法：
#   sh scripts/board/usb_gamepad/setup_hid_gamepad.sh           # 添加或更新 HID 功能
#   sh scripts/board/usb_gamepad/setup_hid_gamepad.sh --remove  # 移除 HID 功能

set -e

G0_DIR="/sys/kernel/config/usb_gadget/g0"
LOG_FILE="/tmp/hid_setup.log"

# HID Report Descriptor (7 bytes) - PS4/PS5 风格:
#   Byte 0-1: 16 buttons (bit flags, 2 bytes)
#   Byte 2: X axis (Left Stick X = Rudder, -127~127)
#   Byte 3: Y axis (Left Stick Y = Throttle, -127~127)
#   Byte 4: Z axis (Right Stick X = Aileron, -127~127)
#   Byte 5: Rx axis (Right Stick Y = Elevator, -127~127)
#   Byte 6: Reserved (padding byte)
HID_REPORT_LENGTH=7
# 16 buttons (29 10 = Button Max 16), then X(30), Y(31), Z(32), Rx(33) axes (原始顺序)
# 75 08 95 01 81 01 = 1 byte constant padding
HID_REPORT_DESC="05 01 09 05 A1 01 05 09 19 01 29 10 15 00 25 01 75 01 95 10 81 02 05 01 09 30 09 31 09 32 09 33 15 81 25 7F 75 08 95 04 81 02 75 08 95 01 81 01 C0"
log() {
    echo "$1"
    echo "$(date '+%H:%M:%S') $1" >> "$LOG_FILE"
}

setup_hid() {
    if [ ! -d "$G0_DIR" ]; then
        log "Error: g0 gadget not found at $G0_DIR"
        exit 1
    fi

    cd "$G0_DIR"
    
    # 记录当前 UDC
    CURRENT_UDC=$(cat UDC 2>/dev/null)
    log "Current UDC: $CURRENT_UDC"
    
    # 查找配置目录
    CONFIG_DIR=$(ls -1d configs/c.* 2>/dev/null | head -n 1)
    if [ -z "$CONFIG_DIR" ]; then
        log "Error: No configuration found in g0"
        exit 1
    fi
    log "Using config: $CONFIG_DIR"
    
    # 检查是否需要更新
    NEED_UPDATE=0
    if [ ! -d "functions/hid.usb0" ]; then
        log "HID function does not exist, will create"
        NEED_UPDATE=1
    elif [ "$(cat functions/hid.usb0/report_length 2>/dev/null)" != "$HID_REPORT_LENGTH" ]; then
        log "HID report length mismatch, will update"
        NEED_UPDATE=1
    fi
    
    if [ "$NEED_UPDATE" = "0" ]; then
        log "HID already configured correctly"
        if [ -c /dev/hidg0 ]; then
            log "✓ /dev/hidg0 exists"
        else
            log "⚠ /dev/hidg0 not found, may need reconnect"
        fi
        return 0
    fi
    
    log ">>> 开始配置 HID，SSH 将暂时断开..."
    
    # 1. 禁用 gadget
    log "[1/5] Disabling gadget..."
    echo "" > UDC
    sleep 1
    
    # 2. 移除旧的 HID 链接
    log "[2/5] Removing old HID link..."
    rm -f "$CONFIG_DIR/hid.usb0" 2>/dev/null || true
    
    # 3. 删除并重建 HID 功能（确保描述符正确）
    log "[3/5] Creating HID function..."
    if [ -d "functions/hid.usb0" ]; then
        rmdir "functions/hid.usb0" 2>/dev/null || true
    fi
    mkdir -p functions/hid.usb0
    echo 0 > functions/hid.usb0/protocol
    echo 0 > functions/hid.usb0/subclass
    echo $HID_REPORT_LENGTH > functions/hid.usb0/report_length
    echo "$HID_REPORT_DESC" | xxd -r -ps > functions/hid.usb0/report_desc
    
    # 4. 链接到配置
    log "[4/5] Linking HID to config..."
    ln -s functions/hid.usb0 "$CONFIG_DIR/"
    
    # 5. 重新启用 gadget
    log "[5/5] Re-enabling gadget..."
    echo "$CURRENT_UDC" > UDC
    sleep 2
    
    # 验证
    if [ -c /dev/hidg0 ]; then
        log "✓ SUCCESS: /dev/hidg0 created"
    else
        log "⚠ WARNING: /dev/hidg0 not found"
    fi
    
    log "=== HID Setup Complete ==="
}

remove_hid() {
    if [ ! -d "$G0_DIR" ]; then
        log "Error: g0 gadget not found"
        exit 1
    fi

    cd "$G0_DIR"
    
    CURRENT_UDC=$(cat UDC 2>/dev/null)
    CONFIG_DIR=$(ls -1d configs/c.* 2>/dev/null | head -n 1)
    
    log ">>> 移除 HID，SSH 将暂时断开..."
    
    echo "" > UDC
    sleep 1
    
    rm -f "$CONFIG_DIR/hid.usb0" 2>/dev/null || true
    rmdir "functions/hid.usb0" 2>/dev/null || true
    
    echo "$CURRENT_UDC" > UDC
    sleep 2
    
    log "=== HID Removed ==="
}

# 主程序
rm -f "$LOG_FILE"
log "=== USB HID Gamepad Setup ==="
log "Log file: $LOG_FILE"

if [ "$1" = "--remove" ]; then
    log "Mode: Remove HID"
    # 后台运行
    (sleep 1; remove_hid) &
    echo "正在后台移除 HID，SSH 可能断开..."
    echo "10 秒后重连，查看日志: cat $LOG_FILE"
else
    log "Mode: Setup HID"
    # 后台运行
    (sleep 1; setup_hid) &
    echo "正在后台配置 HID，SSH 可能断开..."
    echo "10 秒后重连，查看日志: cat $LOG_FILE"
fi

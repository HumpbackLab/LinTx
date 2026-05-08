#!/bin/sh
# LinTx HID Gamepad 开机自启脚本
# 用来代替 scripts/board/usb_gamepad/setup_hid_gamepad.sh，开机自启，不用再手动运行
# 安装: cp scripts/board/usb_gamepad/init_hid_gamepad.sh /etc/init.d/ && chmod +x /etc/init.d/init_hid_gamepad.sh
# 自动运行：mv /etc/init.d/init_hid_gamepad.sh /etc/rc.d/S99_hid_gamepad.sh

G0_DIR="/sys/kernel/config/usb_gadget/g0"

# HID Report Descriptor (7 bytes): 16 buttons + 4 axes + padding
HID_REPORT_LENGTH=7
HID_REPORT_DESC="05 01 09 05 A1 01 05 09 19 01 29 10 15 00 25 01 75 01 95 10 81 02 05 01 09 30 09 31 09 32 09 33 15 81 25 7F 75 08 95 04 81 02 75 08 95 01 81 01 C0"

setup_hid() {
    [ ! -d "$G0_DIR" ] && return 1
    cd "$G0_DIR"
    
    # 如果 HID 已存在且长度正确，直接返回
    if [ -d "functions/hid.usb0" ]; then
        [ "$(cat functions/hid.usb0/report_length 2>/dev/null)" = "$HID_REPORT_LENGTH" ] && return 0
    fi
    
    # 记录当前 UDC
    CURRENT_UDC=$(cat UDC 2>/dev/null)
    CONFIG_DIR=$(ls -1d configs/c.* 2>/dev/null | head -n 1)
    [ -z "$CONFIG_DIR" ] && return 1
    
    # 禁用 gadget
    echo "" > UDC
    sleep 1
    
    # 移除旧 HID
    rm -f "$CONFIG_DIR/hid.usb0" 2>/dev/null
    rmdir "functions/hid.usb0" 2>/dev/null
    
    # 创建 HID 功能
    mkdir -p functions/hid.usb0
    echo 0 > functions/hid.usb0/protocol
    echo 0 > functions/hid.usb0/subclass
    echo $HID_REPORT_LENGTH > functions/hid.usb0/report_length
    echo "$HID_REPORT_DESC" | xxd -r -ps > functions/hid.usb0/report_desc
    
    # 链接到配置
    ln -s functions/hid.usb0 "$CONFIG_DIR/"
    
    # 重新启用 gadget
    echo "$CURRENT_UDC" > UDC
    sleep 1
    
    return 0
}

case "$1" in
    start)
        echo "Setting up HID gamepad..."
        setup_hid && echo "HID gamepad ready: /dev/hidg0" || echo "HID setup failed"
        ;;
    stop)
        # 可选：移除 HID (一般不需要)
        echo "HID stop not implemented"
        ;;
    *)
        echo "Usage: $0 {start|stop}"
        exit 1
        ;;
esac

exit 0

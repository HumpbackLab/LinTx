#set text(
  font: ("Noto Sans", "Source Han Sans SC", "Microsoft YaHei"),
  size: 11pt,
  lang: "zh",
  region: "cn",
)

#set par(leading: 0.8em, spacing: 1.2em)

#set page(
  paper: "a4",
  margin: (x: 2.5cm, y: 2.5cm),
  header: context {
    if counter(page).get().first() > 1 [
      #set text(8pt, gray)
      #grid(
        columns: (1fr, 1fr),
        [LinTx 用户手册],
        align(right)[版本: v0.1.0]
      )
      #v(-0.5em)
      #line(length: 100%, stroke: 0.5pt + gray)
    ]
  },
  footer: context [
    #align(center, text(9pt, gray)[第 #counter(page).display() 页])
  ],
)

#set heading(numbering: "1.1 ")
#show heading: it => {
  v(1.2em, weak: true)
  it
  v(0.6em)
}

#show outline.entry: set par(leading: 1.2em)

// 辅助函数：绘制占位图
#let placeholder(caption, img_path: none, height: 10em, width: 100%) = figure(
  if img_path != none {
    image(img_path, width: width)
  } else {
    rect(width: 100%, height: height, stroke: 1pt + navy, fill: luma(250), radius: 4pt)[
      #align(center + horizon)[
        #text(gray, size: 14pt)[#caption] \
        #v(0.5em)
        #text(gray, size: 9pt)[请在此替换为实物照片或界面截图]
      ]
    ]
  },
  caption: caption,
)

#let caution(body) = block(
  fill: rgb("#fff5f5"),
  stroke: (left: 4pt + red),
  inset: 12pt,
  radius: 4pt,
  width: 100%,
  [*注意：* #body],
)

#let tip(body) = block(
  fill: rgb("#f0f8ff"),
  stroke: (left: 4pt + blue),
  inset: 12pt,
  radius: 4pt,
  width: 100%,
  [*提示：* #body],
)

#let info(body) = block(
  fill: rgb("#f0fff0"),
  stroke: (left: 4pt + green),
  inset: 12pt,
  radius: 4pt,
  width: 100%,
  [*说明：* #body],
)

// 封面
#align(center + horizon)[
  #block(inset: 3em)[
    #text(28pt, weight: "bold", fill: navy)[LinTx] \
    #v(0.4em)
    #text(18pt, weight: "medium")[用户使用手册] \
    #v(1.2em)
    #text(11pt, gray)[基于 Rust 的模块化航模遥控系统]
  ]
  
  #placeholder("产品外观示意图", height: 15em)
  
  #v(1fr)
  #text(10pt, gray)[文档版本：v0.1.0 | 最后更新：2026年4月] \
  #text(10pt, gray)[适用硬件：SG2002 RISC-V 板卡]
]

#pagebreak()

// 目录
#outline(indent: 2em, depth: 2)

#pagebreak()

= 产品简介 <intro>

== 什么是 LinTx？

*LinTx* 是一个基于 Rust 开发的模块化遥控系统应用，专为航模爱好者和开发者设计。它采用客户端-服务器架构，支持多种输入源、混控逻辑和输出协议，可运行在多种平台上。

LinTx 的核心设计理念是*模块化*与*可扩展性*。每个功能模块（如输入采集、混控处理、ELRS 发射等）都可以独立运行并通过消息机制进行通信，使得系统具有极高的灵活性。

== 核心技术亮点

- *跨平台支持*：同时支持 RISC-V 64 位板卡（SG2002）、x86_64 Linux 和 Windows
- *模块化架构*：基于 `rpos` 运行时库，实现模块间解耦和消息驱动
- *多协议输入*：支持 STM32 串口协议、CRSF 协议、ADC 直采、Linux joydev 等多种输入源
- *ELRS 发射*：完整支持 ExpressLRS 协议，包括参数配置、对频、发射功率调节
- *混控系统*：灵活的机型配置和混控逻辑，支持多种输出协议（CRSF、USB HID、PPM、SBUS）
- *图形界面*：基于 LVGL 的嵌入式 GUI，支持 Framebuffer 和 SDL 后端
- *静态链接*：使用 musl 静态链接，运行时依赖最少，二进制体积小

== 支持的平台

#table(
  columns: (1fr, 2fr),
  inset: 10pt,
  align: horizon,
  
  [*平台*], [*目标架构*],
  [SG2002 板卡], [`riscv64gc-unknown-linux-musl`],
  [Linux 桌面], [`x86_64-unknown-linux-gnu`],
  [Windows 桌面], [`x86_64-pc-windows-gnu`],
)

#pagebreak()

= 系统架构 <architecture>

== 整体架构

LinTx 采用客户端-服务器架构。主程序作为服务器后台运行，各功能模块作为客户端连接到服务器，通过消息通道进行数据交换。

#figure(
  rect(width: 100%, height: 12em, stroke: 1pt + navy, fill: luma(250), radius: 4pt)[
    #align(center + horizon)[
      #text(gray, size: 10pt)[
        ```
        ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
        │  输入模块   │────▶│   混控器    │────▶│  输出模块   │
        │ (stm32/adc) │     │   (mixer)   │     │ (elrs_tx)   │
        └─────────────┘     └─────────────┘     └─────────────┘
              │                   │                   │
              └───────────────────┼───────────────────┘
                                  ▼
                          ┌─────────────┐
                          │  rpos 消息  │
                          │    中心     │
                          └─────────────┘
                                  ▲
                                  │
                          ┌─────────────┐
                          │   UI 界面   │
                          │  (ui_demo)  │
                          └─────────────┘
        ```
      ]
    ]
  ],
  caption: [LinTx 系统架构示意图],
)

== 核心模块

=== 输入模块
- *stm32_serial*：读取 STM32 采集的摇杆 ADC 数据（推荐主输入链）
- *crsf_rc_in*：接收外部 CRSF 遥控数据（兼容输入源）
- *adc*：直接读取 ADS1115 ADC 数据
- *mock_joystick*：模拟摇杆数据生成器（测试用途）

=== 处理模块
- *mixer*：混控器，处理输入数据并应用机型配置
- *calibrate*：摇杆校准模块

=== 输出模块
- *elrs_tx*：ELRS 发射模块
- *elrs_agent*：ELRS 配置状态服务
- *usb_gamepad*：USB HID 手柄输出

=== 界面模块
- *ui_demo*：LVGL 图形界面入口
- *system_state_mock*：系统状态数据源

#pagebreak()

= 硬件准备 <hardware>

== SG2002 板卡

LinTx 主要针对 SG2002 RISC-V 64 位板卡开发。该板卡具有以下特点：

- RISC-V 64 位处理器
- 支持 Linux 操作系统（Buildroot/BusyBox）
- 多路 UART 串口
- I2C 接口（用于 ADC）
- Framebuffer 显示输出

#placeholder("SG2002 板卡实物图", height: 10em)

== 接口说明

#table(
  columns: (1fr, 1.5fr, 2fr),
  inset: 8pt,
  align: horizon + center,
  
  [*接口*], [*典型设备*], [*用途*],
  [UART (ttyS0)], [STM32 串口], [摇杆数据输入],
  [UART (ttyS2)], [ELRS 模块], [RF 发射],
  [I2C (i2c-0)], [ADS1115], [ADC 数据采集],
  [Framebuffer], [显示屏], [GUI 显示],
  [触摸/按键], [输入设备], [用户交互],
)

== 显示配置

板端 Framebuffer 默认参数：
- 分辨率：800x480
- 旋转角度：`LINTX_FB_ROTATE=270`
- 颜色交换：`LINTX_FB_SWAP_RB=1`

#pagebreak()

= 软件安装与编译 <installation>

== 编译环境准备

=== 主机编译（Linux/Windows）

```bash
# 1. 安装 Rust 工具链
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 验证默认 Linux 主机编译
cargo check

# 3. Windows 目标（首次需要安装 target）
rustup target add x86_64-pc-windows-gnu
cargo check --target x86_64-pc-windows-gnu
```

=== 交叉编译（SG2002 板卡）

```bash
# 安装 cross 工具
cargo install cross

# 编译板端 GUI 包
cross build --target riscv64gc-unknown-linux-musl --release --features lvgl_ui
```

编译完成后，二进制位于 `target/riscv64gc-unknown-linux-musl/release/LinTx`。

== 可选功能特性

LinTx 支持多个可选功能特性（features）：

#table(
  columns: (1fr, 2fr),
  inset: 10pt,
  align: horizon,
  
  [*特性*], [*说明*],
  [`lvgl_ui`], [启用 LVGL 图形界面（板端必需）],
  [`sdl_ui`], [启用 PC SDL 窗口后端（桌面调试）],
  [`joydev_input`], [启用 Linux joydev 输入模块],
  [`lua`], [启用 Lua 脚本运行时],
)

编译示例：
```bash
# 启用 SDL UI（桌面调试）
cargo check --features sdl_ui

# 启用 Lua 脚本支持
cargo check --features lua

# 启用 joydev 输入
cargo check --features joydev_input
```

#pagebreak()

= 快速上手 <quickstart>

== 基本用法

LinTx 的基本命令格式为：

```bash
./LinTx -- <模块名称> [模块参数]
```

== 启动服务器

在运行任何模块之前，首先需要启动服务器：

```bash
./LinTx --server &
```

== 典型使用场景

=== 场景 1：模拟测试（无硬件）

```bash
# 启动服务器
./LinTx --server &

# 启动模拟摇杆数据
./LinTx -- mock_joystick &

# 启动混控器
./LinTx -- mixer &

# 启动 UI（SDL 后端）
./LinTx -- ui_demo --backend sdl --width 800 --height 480 --fps 30
```

=== 场景 2：板端真实 ELRS 发射

```bash
# 启动服务器
./LinTx --server &

# 启动 STM32 串口输入
./LinTx -- stm32_serial /dev/ttyS0 --baudrate 115200 &

# 启动混控器
./LinTx -- mixer &

# 启动 ELRS 发射
./LinTx -- elrs_tx /dev/ttyS2 --baudrate 115200 &

# 启动 UI（Framebuffer 后端）
LINTX_FB_ROTATE=270 LINTX_FB_SWAP_RB=1 \
  ./LinTx -- ui_demo --backend fb --fb-device /dev/fb0 --width 800 --height 480
```

=== 场景 3：USB 手柄模式

```bash
# 配置 USB Gadget
sh scripts/board/usb_gamepad/setup_hid_gamepad.sh

# 启动数据流
./LinTx --server &
./LinTx -- mock_joystick &
./LinTx -- mixer &
./LinTx -- usb_gamepad
```

#pagebreak()

= 模块详解 <modules>

== 输入模块

=== stm32_serial

读取 STM32 发送的自定义协议摇杆数据（0x5A 头）。这是当前 TX 主机方案的推荐输入链。

*参数：*
- `<设备路径>`：串口设备路径（必选）
- `--baudrate <波特率>`：串口波特率，默认 115200

*示例：*
```bash
./LinTx -- stm32_serial /dev/ttyS0 --baudrate 115200
```

=== crsf_rc_in

读取外部 CRSF 遥控数据。适合接上游 CRSF 发送端或外部测试源。

*参数：*
- `<设备路径>`：串口设备路径（必选）
- `--baudrate <波特率>`：默认 420000

*示例：*
```bash
./LinTx -- crsf_rc_in /dev/ttyS0
```

=== adc

读取 ADS1115 ADC 数据（硬编码使用 `/dev/i2c-0`）。

*示例：*
```bash
./LinTx -- adc
```

=== mock_joystick

模拟摇杆数据生成器，用于测试目的。支持三种模式：

#table(
  columns: (1fr, 2fr),
  inset: 10pt,
  align: horizon,
  
  [*模式*], [*说明*],
  [`static`], [发送固定的通道值],
  [`sine`], [发送正弦波振荡的通道值],
  [`step`], [发送离散的阶跃值],
)

*参数：*
- `--config <配置文件路径>`：默认 `mock_config.toml`

*配置文件示例：*
```toml
mode = "static"
update_rate_hz = 50

[static_config]
channels = [992, 992, 0, 992]

[sine_config]
base = [992, 992, 0, 992]
amplitude = [200, 150, 0, 100]
frequency_hz = [1.0, 0.5, 0.0, 2.0]

[step_config]
values = [
    [0, 0, 0, 0],
    [992, 992, 0, 992],
    [1984, 1984, 1984, 1984]
]
step_duration_ms = 2000
```

== 混控器

=== mixer

处理输入数据并进行混控逻辑。依赖 `joystick.toml` 校准文件和机型配置。

混控器将原始输入数据转换为标准输出值（0-10000），并应用机型配置中的权重、偏移和限制。

*通道映射：*
- Thrust（油门）
- Direction（方向/偏航）
- Aileron（副翼/横滚）
- Elevator（升降/俯仰）

== 输出模块

=== elrs_tx

驱动 ELRS 发射高频头。

*参数：*
- `<设备路径>`：串口设备路径（必选）
- `--baudrate <波特率>`：默认 115200

=== elrs_agent

ELRS 配置状态服务，为 UI 提供 ELRS 模块状态、参数列表和交互命令。

*模式：*
- `mock`：无硬件演示 UI 交互
- `crsf`：通过 UART 发送 CRSF 命令，解析设备信息

*参数：*
- `--mode <mock|crsf>`：默认 mock
- `--dev-name <设备路径>`：默认 `/dev/ttyS3`
- `--baudrate <波特率>`：默认 420000

=== usb_gamepad

将混控后的数据输出到 USB HID 手柄设备。

*前置条件：* 板端 `start` 会自动尝试运行 `scripts/board/usb_gamepad/setup_hid_gamepad.sh` 配置 USB Gadget；手工验证时也可直接运行该脚本。

*通道映射：*
- thrust → HID Rz 轴
- direction → HID X 轴
- aileron → HID Z 轴
- elevator → HID Y 轴

#pagebreak()

= 图形界面 <ui>

== UI 后端

LinTx 的 UI 系统基于 LVGL，支持多种后端：

#table(
  columns: (1fr, 2fr),
  inset: 10pt,
  align: horizon,
  
  [*后端*], [*说明*],
  [`sdl`], [PC SDL 窗口后端（开发调试）],
  [`fb`], [板卡 Framebuffer 后端],
  [`pc`], [PC 终端后端（纯文本）],
)

== 启动 UI

```bash
# SDL 后端（PC 调试）
./LinTx -- ui_demo --backend sdl --width 800 --height 480 --fps 30

# Framebuffer 后端（板端）
LINTX_FB_ROTATE=270 LINTX_FB_SWAP_RB=1 \
  ./LinTx -- ui_demo --backend fb --fb-device /dev/fb0 --width 800 --height 480
```

== 界面导航

=== 键盘操作

#table(
  columns: (1fr, 2fr),
  inset: 10pt,
  align: horizon,
  
  [*按键*], [*功能*],
  [←/→], [左右移动 / 切换页面],
  [↑/↓], [上下移动 / 调整数值],
  [Enter], [进入应用 / 确认],
  [Esc], [返回上级],
  [Q], [退出程序],
)

=== Launcher 页面

主页面采用横排应用布局，支持多页切换。第一页为 1x4 布局，后续页面可扩展为 2x4 布局。

#placeholder("Launcher 界面截图", height: 10em)

== 应用页面

=== SYSTEM（系统设置）

显示系统状态信息，支持调整背光和音量。

*操作：*
- ↑/↓：调整背光亮度
- ←/→：调整音量

*显示信息：*
- 遥控电量
- 飞行器电量
- 信号强度
- 系统时间
- 背光/音量设置

=== CONTROL（控制监视）

实时查看输入链路数据和混控输出。用于验证输入链是否正常工作。

*显示信息：*
- 输入源类型
- 链路状态
- ELRS 反馈（连接状态、信号强度、飞机电量）
- 原始通道值
- 混控输出值（Thrust、Direction、Aileron、Elevator）

=== MODELS（机型选择）

管理和切换飞行器机型配置。

*操作：*
- ↑/↓：选择机型
- Enter：应用当前选中的机型

*预置机型：*
- Quad X（四轴 X 布局）
- Fixed Wing（固定翼）
- Rover（地面车）

=== ELRS（RF 链路配置）

ELRS 模块参数配置和对频操作。

*功能：*
- 手动 WiFi 开关
- 对频模式
- 发射功率调节（10/25/100/250/500/1000mW）
- Bind Phrase 设置

*操作：*
- ↑/↓：选择参数
- ←/→：调整参数值
- Enter：激活/保存
- \]：刷新参数

=== USB PAD（USB 手柄输出）

从 UI 中查看 `/dev/hidg0` 状态，并开关 `usb_gamepad`，将当前 mixer 输出写入 HID 设备。

*操作：*
- Enter：切换 USB 手柄输出 ON/OFF

=== CLOUD（云同步）

云端数据同步功能（开发中）。

*操作：*
- Enter：切换在线/离线状态

=== ABOUT（关于）

显示系统版本信息。

#pagebreak()

= 配置文件 <configuration>

== radio.toml

全局遥控器配置文件，包含 UI、音频、输入和 ELRS 设置。

```toml
schema_version = 1
active_model = "quad_x"

[ui]
backlight_percent = 70
theme = "classic"

[audio]
sound_percent = 60
mute = false

[input]
calibration_profile = "joystick.toml"
source_priority = ["adc", "crsf", "mock"]

[elrs]
wifi_manual_on = false
bind_mode = false
tx_power_mw = 100
bind_phrase = "654321"
```

== joystick.toml

摇杆校准配置文件，定义各通道的映射和范围。

```toml
channel_indexs = [1, 0, 2, 3]

[[channel_infos]]
name = "Thrust"
index = 1
min = 0
max = 4095
rev = false

[[channel_infos]]
name = "Direction"
index = 0
min = 0
max = 4095
rev = true

[[channel_infos]]
name = "Aileron"
index = 2
min = 0
max = 4095
rev = true

[[channel_infos]]
name = "Elevator"
index = 3
min = 0
max = 4095
rev = false
```

== 机型配置文件

机型配置文件位于 `models/` 目录，每个机型一个 `.toml` 文件。

=== 配置结构

```toml
id = "quad_x"
name = "Quad X"

# 输入映射
[[input_mapping.channels]]
role = "thrust"
source = "adc"
index = 0
reversed = false

# 混控输出
[[mixer.outputs]]
role = "thrust"
weight = 100
offset = 0
curve = "linear"

[mixer.outputs.limits]
min = -1000
max = 1000
subtrim = 0
reversed = false

# 输出协议
[output]
protocol = "crsf"
channel_order = ["aileron", "elevator", "thrust", "direction"]
failsafe = [0, 0, 0, 0]

# 遥测配置
[telemetry]
enabled = true

[[telemetry.sensors]]
key = "rssi"
unit = "percent"
enabled = true

# 飞行配置
[[profiles]]
name = "acro"
roll_rate = 220
pitch_rate = 220
yaw_rate = 180
expo_percent = 20
```

=== 输出协议

#table(
  columns: (1fr, 2fr),
  inset: 10pt,
  align: horizon,
  
  [*协议*], [*说明*],
  [`crsf`], [Crossfire 协议（ELRS 使用）],
  [`usb_hid`], [USB HID 手柄],
  [`ppm`], [PPM 信号],
  [`sbus`], [SBUS 协议],
)

#pagebreak()

= 摇杆校准 <calibration>

== 校准流程

LinTx 提供交互式摇杆校准工具。校准过程会自动识别各通道并记录行程范围。

```bash
./LinTx --server &
./LinTx -- adc &
./LinTx -- calibrate
```

== 校准步骤

1. *居中*：将所有摇杆推到中位，等待 5 秒
2. *通道识别*：依次将每个通道推到最低（或最左），系统自动识别
   - Thrust（油门）
   - Direction（方向）
   - Aileron（副翼）
   - Elevator（升降）
3. *行程采集*：在 10 秒内将所有摇杆转动到极限位置
4. *完成*：校准数据自动保存到 `joystick.toml`

#tip[
  校准时请确保摇杆能够到达完整行程。如果校准后发现某个通道方向反转，可以在 `joystick.toml` 中设置 `rev = true`。
]

#pagebreak()

= Lua 脚本支持 <lua>

== 概述

LinTx 支持 Lua 脚本扩展（需编译时启用 `--features lua`）。Lua 脚本可以直接操作串口和 CRSF 协议。

== 启用 Lua

```bash
cargo run --features lua -- -- lua_run <脚本路径> [参数...]
```

== 全局 API

=== 串口操作

```lua
-- 打开串口
local port = uart_open("/dev/ttyS3", 420000)

-- 写入原始字节
port:write(data)

-- 按十六进制写入
port:write_hex("ee 06 2d ee ea 01 00 67")

-- 读取数据
local data = port:read(max_len, timeout_ms)
local hex_data = port:read_hex(max_len, timeout_ms)
```

=== CRSF 协议

```lua
-- 封装通用 CRSF 帧
local frame = crsf.encode(dest, frame_type, payload)

-- 封装 RC 通道帧
local rc_frame = crsf.rc_channels({992, 992, 0, 992, ...})
```

=== 工具函数

```lua
-- 十六进制转换
local bytes = bytes_from_hex("C8 07 32")
local hex_str = hex(bytes)

-- 延时
sleep_ms(100)

-- 日志
log("message")
```

== 脚本示例

```lua
-- ELRS Magic 命令发送示例
local port = uart_open(ARGS[1] or "/dev/ttyS3", 420000)

-- 发送 ELRS BIND 命令
port:write_hex("C8 07 32 EE EA 10 01 14 EB")

sleep_ms(100)

-- 读取响应
local response = port:read_hex(64, 1000)
log("Response: " .. (response or "timeout"))
```

#pagebreak()

= 板端部署 <deployment>

== 部署脚本

板端推荐使用仓库根目录的 `start` 启动完整功能链路；专项验证脚本位于 `scripts/board/tests/`。

=== start

启动默认完整链路：
```bash
./start
```

无 STM32 输入时，可使用 mock 输入：
```bash
./start /dev/ttyS2 115200 mock
```

=== test_elrs_ui_config.sh

启动完整的输入+混控+RF+UI 流程：
```bash
sh ./scripts/board/tests/test_elrs_ui_config.sh /dev/ttyS2 115200 stm32 /dev/ttyS0 115200
```

=== test_input_stm32.sh

验证 STM32 输入链：
```bash
sh ./scripts/board/tests/test_input_stm32.sh /dev/ttyS3 115200
```

=== stop_lintx.sh

停止所有 LinTx 进程：
```bash
sh ./scripts/board/stop_lintx.sh
```

== 环境变量

#table(
  columns: (1fr, 1fr, 2fr),
  inset: 8pt,
  align: horizon,
  
  [*变量*], [*默认值*], [*说明*],
  [`LINTX_FB_ROTATE`], [270], [Framebuffer 旋转角度],
  [`LINTX_FB_SWAP_RB`], [1], [红蓝通道交换],
  [`LINTX_ELRS_DEBUG`], [0], [ELRS 调试日志],
)

#pagebreak()

= 常见问题排查 <faq>

== UI 相关

/ 问：ui_demo --backend sdl 启动后白屏？:
答：请完整重启 server 与 UI 相关进程，确保按顺序启动：server → 数据源 → ui_demo。

/ 问：UI 页面出现滚动条？:
答：请使用最新版本，应用页容器已默认关闭滚动条。

/ 问：文字显示为方框？:
答：当前 UI 默认使用 ASCII 字符。如使用中文，请确认字体配置正确。

== 输入相关

/ 问：摇杆数据不正常？:
答：
1. 检查串口设备路径是否正确
2. 检查波特率是否匹配
3. 运行 `calibrate` 模块重新校准

/ 问：校准后通道方向反转？:
答：在 `joystick.toml` 中将对应通道的 `rev` 设置为 `true`。

== ELRS 相关

/ 问：无法对频？:
答：
1. 确认发射端和接收端固件版本兼容（V2/V3 不互通）
2. 确认 Bind Phrase 完全一致
3. 检查发射功率设置

/ 问：ELRS 页面显示 "Not Connected"？:
答：确认 `elrs_agent` 或 `rf_link_service` 正在运行，且串口参数正确。

== 系统相关

/ 问：如何停止所有 LinTx 进程？:
答：
```bash
# 使用停止脚本
sh scripts/board/stop_lintx.sh

# 或手动停止
pkill -f "LinTx.*--server" || true
pkill -f "LinTx.*ui_demo" || true
pkill -f "LinTx" || true
```

#pagebreak()

= 开发者参考 <developer>

== rpos 运行时

LinTx 的模块化架构基于 `rpos` 库，提供以下核心功能：

- *高精度时钟*：用于调度线程或工作队列
- *消息通道*：进程间通信，只保留最新消息
- *定时线程*：周期性调度
- *模块系统*：模块注册和获取

== 消息类型

主要消息类型定义在 `src/messages.rs`：

- `AdcRawMsg`：ADC 原始数据
- `InputFrameMsg`：输入帧数据
- `MixerOutMsg`：混控输出
- `SystemStatusMsg`：系统状态
- `SystemConfigMsg`：系统配置
- `ElrsStateMsg`：ELRS 状态
- `ActiveModelMsg`：当前机型

== 添加新模块

1. 在 `src/` 下创建模块文件
2. 实现模块主函数
3. 使用 `#[rpos::ctor::ctor]` 注册模块

```rust
fn my_module_main(_argc: u32, _argv: *const &str) {
    // 模块逻辑
}

#[rpos::ctor::ctor]
fn register() {
    rpos::module::Module::register("my_module", my_module_main);
}
```

== 测试

```bash
# 运行单元测试
cargo test

# 运行特定测试
cargo test test_mixer
```

#pagebreak()

= 附录 <appendix>

== 通道值范围

#table(
  columns: (1fr, 1fr, 1fr),
  inset: 10pt,
  align: horizon + center,
  
  [*类型*], [*范围*], [*中心值*],
  [ADC 原始值], [0 - 4095], [2048],
  [CRSF 通道值], [172 - 1811], [992],
  [混控输出值], [0 - 10000], [5000],
  [HID 轴值], [-127 - 127], [0],
)

== 许可证

本项目遵循 MIT 许可证。

== 相关链接

- 项目仓库：https://github.com/...
- ExpressLRS：https://www.expresslrs.org/
- LVGL：https://lvgl.io/

== 更新日志

*v0.1.0*
- 初始版本
- 支持 SG2002 RISC-V 板卡
- 完整的输入-混控-输出链路
- LVGL 图形界面
- ELRS 发射支持

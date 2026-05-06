# LinTx 项目说明

## 项目简介
LinTx 是一个基于 Rust 的模块化遥控系统应用，当前同时支持：
- 板卡目标：**RISC-V 64 位**（`riscv64gc-unknown-linux-musl`，Buildroot/BusyBox，SG2002）
- 桌面目标：**x86_64 Linux**
- 桌面目标：**x86_64 Windows**（`x86_64-pc-windows-gnu`）

## 关键特性
- 使用 **musl** 静态链接，二进制体积小，运行时依赖最少。
- 通过 **cross** + 自定义 Docker 镜像（包含 binutils 2.42）实现跨编译。
- 模块化设计，支持多种协议输入（如 STM32 串口、CRSF）和功能模块（如 ADC 读取、混控器、ELRS 发射）。

## 默认按键/开关映射
LinTx 遥控器端只采样物理输入、做安全保护并输出 ELRS/CRSF 通道，不实现 Angle、Horizon、Turtle、GPS Rescue/RTL 等飞控功能。Betaflight、ArduPilot、INAV 或 PX4 端需要自行把 AUX 通道范围绑定到对应模式。

默认模型的通道语义：
- `CH1` Roll / Aileron：右摇杆 X
- `CH2` Pitch / Elevator：右摇杆 Y
- `CH3` Throttle / Thrust：左摇杆 Y
- `CH4` Yaw / Direction：左摇杆 X
- `CH5` ARM：左肩两段自锁，`0=DISARM`，`10000=ARM`
- `CH6` Flight Mode：正面三段 1，`0=Acro`，`5000=Angle`，`10000=Horizon`
- `CH7` Beeper：正面三段 2，`0=Off`，`5000=Reserved`，`10000=Beeper`
- `CH8` Turtle：正面三段 3，`0=Off`，`5000=Reserved`，`10000=Turtle`
- `CH9` PreArm：正面三段 4，`0=Off`，`5000=PreArm`，`10000=Reserved`
- `CH10` GPS Rescue / RTL：右肩两段自锁，`0=Off`，`10000=Rescue`
- `CH11` Reserved
- `CH12` Reserved
- `CH13-CH16` Reserved / `0`

安全默认值：没有明确开关输入时 `CH5 ARM`、`CH8 Turtle`、`CH10 GPS Rescue/RTL` 均保持 `0`；油门不低时 mixer 会强制 `CH5=0`。

## 编译步骤
```bash
# 1. 清理旧的构建产物
cargo clean

# 2. 编译当前主机（x86_64 Linux）
cargo check

# 3. 编译 Windows 目标（首次需要安装 target）
rustup target add x86_64-pc-windows-gnu
cargo check --target x86_64-pc-windows-gnu

# 4. 使用 cross 编译 RISC-V 板端 GUI 包（已配置自定义镜像）
cross build --target riscv64gc-unknown-linux-musl --release --features lvgl_ui
```
编译完成后，二进制位于 `target/riscv64gc-unknown-linux-musl/release/LinTx`。
注意：板端 `fb` UI 依赖 `lvgl_ui` feature；如果不带这个 feature，`ui_demo --backend fb` 会退化成文本终端后端，不会打开 `/dev/fb0`。

### 可选功能
- `joydev_input`：启用 Linux `joydev` 输入模块
```bash
cargo check --features joydev_input
```
- `sdl_ui`：启用 PC SDL 窗口 UI 后端（WSL2/桌面调试建议开启）
```bash
cargo check --features sdl_ui
```
- `lua`：启用 Lua 脚本运行时（串口/UART 与 CRSF 辅助 API）
```bash
cargo check --features lua
```

## 如何使用

LinTx 采用客户端-服务器架构（基于 `rpos` 库）。主程序通常作为服务器后台运行，通过命令行参数启动具体的子模块。

### 板端启动脚本
板端推荐使用 `scripts/board/` 下的 `sh` 脚本启动 GUI 和 ELRS 联调流程。

部署完成后，在板子上执行：

```bash
cd /root/lintx
sh ./scripts/board/test_gui_mock.sh
sh ./scripts/board/test_gui_crsf.sh /dev/ttyS3 420000
sh ./scripts/board/test_gui_crsf_debug.sh /dev/ttyS3 420000
sh ./scripts/board/test_elrs_ui_config.sh /dev/ttyS2 115200 stm32 /dev/ttyS0 115200
sh ./scripts/board/test_input_mock.sh
sh ./scripts/board/test_input_stm32.sh /dev/ttyS3 115200
sh ./scripts/board/stop_lintx.sh
```

说明：
- `test_gui_mock.sh`：启动 `server + elrs_agent(mock) + ui_demo(fb)`。
- `test_gui_crsf.sh`：启动真实 ELRS/CRSF GUI 联调流程。
- `test_gui_crsf_debug.sh`：在真实流程上额外打开 `LINTX_ELRS_DEBUG=1`。
- `test_elrs_ui_config.sh`：启动 `server + stm32_serial/mock + mixer + rf_link_service + ui_demo`，用于 ELRS 页面参数和 Bind 联调。推荐 `RF UART @ 115200`。
- `test_input_mock.sh`：启动 `server + mock_joystick + mixer + ui_demo(fb)`，用于输入链验证。
- `test_input_stm32.sh`：启动 `server + stm32_serial + mixer + ui_demo(fb)`，用于当前 TX 主输入链验证。
- `stop_lintx.sh`：停止板上的 `LinTx` 进程并清理 socket。
- 板端 framebuffer 默认参数已经固化在脚本里：`800x480` 逻辑界面、`LINTX_FB_ROTATE=270`、`LINTX_FB_SWAP_RB=1`。
- `rf_link_service` 需要持续输入流（`stm32_serial` 或 `mock_joystick`）来持续发送 RC 通道；只有 bind 命令帧通常不足以建立稳定链路。

### 基本用法（Linux）
```bash
# 格式
./LinTx -- <模块名称> [模块参数]
```

### 基本用法（Windows）
Windows 下使用本地模式（不依赖 Unix socket server）：
```bash
LinTx -- <模块名称> [模块参数]
```

### 可用模块及参数

#### 1. `stm32_serial` (STM32 自定义串口协议)
用于读取 STM32 发送的自定义协议摇杆/输入数据（0x5A 头）。
这条链路更符合当前 TX 主机方案：`STM32 采 ADC -> Linux 读取串口 -> mixer -> ELRS 发射`。
- **参数**:
  - `<设备路径>`: (必选，位置参数) 串口设备路径，例如 `/dev/ttyS0`。
  - `--baudrate <波特率>`: (可选) 串口波特率，默认 `115200`。
- **示例**:
  ```bash
  ./LinTx -- stm32_serial /dev/ttyS0 --baudrate 115200
  ```
- **报文格式**:
  - 旧帧：`5A 0C 01 CH1_L CH1_H CH2_L CH2_H CH3_L CH3_H CH4_L CH4_H SW SH CRC`
  - 新帧：`5A 0E 01 CH1_L CH1_H CH2_L CH2_H CH3_L CH3_H CH4_L CH4_H SW SH BTN_L BTN_H CRC`
  - `CH1..CH4`、`Buttons` 均为 little-endian；`CRC` 为 CRC-8/DVB-S2，覆盖 payload 去掉最后 CRC 的部分。
  - `SW`：bit0-1/2-3/4-5/6-7 分别为 4 个正面三段开关，值 `0/1/2`。
  - `SH`：bit0 为左肩两段，bit1 为右肩两段。
  - `Buttons`：bit0-4 为五向键1，bit5-9 为五向键2，bit10-14 为 GUI 五向键，上/下/左/右/按下，bit15 保留。

#### 1.1 `rc_button_input` (RC 按键转 UI 事件)
订阅 `rc_input_raw`，把 GUI 五向键转换为现有 `ui_input_event`，保持 STM 协议解析和 GUI 解耦。五向键1/2 暂时只保留在原始 `buttons` 位图中，供后续微调功能使用。
- GUI 五向键映射：上/下/左/右 -> `Up/Down/Left/Right`。
- 中键短按 -> `Open`。
- 中键长按或左+中 -> `Back`。
- **示例**:
  ```bash
  ./LinTx -- rc_button_input
  ```

#### 2. `crsf_rc_in` (CRSF 协议输入)
用于读取外部设备送入的标准 CRSF 遥控数据。
这是兼容型输入源，适合接“上游 CRSF 发送端 / 其他遥控系统 / 外部测试源”。
它不是当前 TX 设备的典型主输入链；如果你的摇杆数据来自 STM32 采样，应优先使用 `stm32_serial`。
- **参数**:
  - `<设备路径>`: (必选，位置参数) 串口设备路径。
  - `--baudrate <波特率>`: (可选) 默认 `420000`。
- **示例**:
  ```bash
  ./LinTx -- crsf_rc_in /dev/ttyS0
  ```

#### 3. `elrs_tx` (ELRS 发射模块)
用于驱动 ELRS 发射高频头。
- **参数**:
  - `<设备路径>`: (必选，位置参数) 串口设备路径。
  - `--baudrate <波特率>`: (可选) 默认 `115200`。
- **示例**:
  ```bash
  ./LinTx -- elrs_tx /dev/ttyS1
  ```

#### 3.1 `lua_run` (Lua 脚本模块，需要 `--features lua`)
用于运行 Lua 脚本，并在脚本里直接操作 UART/CRSF。

- **参数**:
  - `<脚本路径>`: (必选) Lua 文件路径。
  - `[脚本参数...]`: (可选) 透传给脚本，全局变量 `ARGS` 可读取。
- **全局 API**:
  - `uart_open(path, baudrate)`：打开串口，返回 `port` 对象。
  - `port:write(data)`：写入原始字节串。
  - `port:write_hex("ee 06 2d ee ea 01 00 67")`：按十六进制文本写入。
  - `port:read(max_len, timeout_ms)` / `port:read_hex(max_len, timeout_ms)`：读取串口数据。
  - `crsf.encode(dest, frame_type, payload)`：封装通用 CRSF 帧。
  - `crsf.rc_channels({...})`：封装 16 通道 RC Channels 帧。
  - `bytes_from_hex(...)` / `hex(...)` / `sleep_ms(...)` / `log(...)`
- **示例**:
  ```bash
  cargo run --features lua -- -- lua_run examples/elrs_magic.lua /dev/ttyS3
  ```

#### 3.2 `elrs_agent` (ELRS 配置状态服务)
用于向 UI 提供 ELRS 模块状态、参数列表和交互命令。目前支持：
- `--mode mock`：无硬件演示 UI 交互。
- `--mode crsf`：通过 UART 发送 CRSF `PING_DEVICES` / `BIND`，解析 `DEVICE_INFO` / `ELRS info`，用于真实 ELRS 模块发现。

- **参数**:
  - `--mode <mock|crsf>`: 默认 `mock`。
  - `--dev-name <设备路径>`: 展示/后续真实串口使用，默认 `/dev/ttyS3`。
  - `--baudrate <波特率>`: 默认 `420000`。
- **示例**:
  ```bash
  # 无硬件联调
  ./LinTx --server &
  ./LinTx -- elrs_agent --mode mock --dev-name /dev/ttyS3 &
  ./LinTx -- ui_demo --backend sdl --width 800 --height 480 --fps 30

  # 板上真实 ELRS/CRSF 模块
  ./LinTx --server &
  ./LinTx -- elrs_agent --mode crsf --dev-name /dev/ttyS3 --baudrate 420000 &
  LINTX_FB_ROTATE=270 LINTX_FB_SWAP_RB=1 ./LinTx -- ui_demo --backend fb --fb-device /dev/fb0 --width 800 --height 480
  ```

#### 4. `adc` (ADC 读取)
读取 ADS1115 ADC 数据（通常用于直接读取摇杆电位器）。
- **参数**: 无（硬编码使用 `/dev/i2c-0`）。
- **示例**:
  ```bash
  ./LinTx -- adc
  ```

#### 5. `mock_joystick` (模拟摇杆数据生成器)
**用于测试目的**：无需物理硬件（不占用IIC/UART），模拟生成摇杆数据用于测试CRSF发送等功能。

- **参数**:
  - `--config <配置文件路径>`: (可选) 配置文件路径，默认 `mock_config.toml`。
- **模式**:
  - **static**: 发送固定的通道值（如居中的摇杆位置）
  - **sine**: 发送正弦波振荡的通道值（用于测试平滑过渡）
  - **step**: 发送离散的阶跃值（用于测试响应）
- **配置文件示例** (`mock_config.toml`):
  ```toml
  mode = "static"  # "static", "sine", "step"
  update_rate_hz = 50

  [static_config]
  channels = [992, 992, 0, 992]  # CRSF居中值
  switch_3pos = [0, 0, 0, 0]
  switch_2pos = [false, false]
  buttons = 0  # low 16 bits follow STM Buttons layout

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
- **示例**:
  ```bash
  # 使用默认配置
  ./LinTx -- mock_joystick
  
  # 使用自定义配置
  ./LinTx -- mock_joystick --config my_mock.toml
  
  # 配合ELRS发射模块测试CRSF发送能力
  ./LinTx --server &
  ./LinTx -- mock_joystick &
  ./LinTx -- elrs_tx /dev/ttyS1
  ```

#### 6. `mixer` (混控器)
处理输入数据并进行混控逻辑（依赖 `joystick.toml` 配置文件）。
- **参数**: 无。
- **示例**:
  ```bash
  ./LinTx -- mixer
  ```

#### 7. `usb_gamepad` (USB HID 手柄输出)
将混控后的数据输出到 USB HID 手柄设备，使从机模拟成 PC 可识别的游戏手柄。
- **前置条件**: 需先运行 `gamepad_composite.sh` 配置 USB Gadget。
- **参数**:
  - `--device <设备路径>`: (可选) HID 设备路径，默认 `/dev/hidg0`。
- **示例**:
  ```bash
  # 1. 配置 USB 复合设备（网络 + 手柄）
  sh gamepad_composite.sh
  
  # 2. 启动完整流程：mock数据 -> mixer -> USB输出
  ./LinTx --server &
  ./LinTx -- mock_joystick &
  ./LinTx -- mixer &
  ./LinTx -- usb_gamepad
  
  # 或使用真实的 STM32 串口输入
  ./LinTx -- stm32_serial /dev/ttyS0 &
  ./LinTx -- mixer &
  ./LinTx -- usb_gamepad
  ```
- **通道映射** (mixer输出 → HID轴):
  - `thrust` (油门) → HID Rz轴
  - `direction` (方向) → HID X轴
  - `aileron` (副翼) → HID Z轴
  - `elevator` (升降) → HID Y轴
  - mixer 值域: 0~10000 (中心值 5000)
  - HID 值域: -127~127 (中心值 0)

#### 8. `system_state_mock` (系统状态/配置模拟源)
用于向 UI 发送基础系统数据：
- 遥控电量
- 飞行器电量
- 信号强度
- 系统时间
- 背光、声音配置

示例：
```bash
./LinTx -- system_state_mock --hz 5
```

#### 9. `ui_demo` (LVGL 应用入口)
这是新的 UI 框架入口，`sdl` 后端已使用真实 LVGL 对象树渲染：

- `--backend sdl`：PC SDL 窗口后端（真实 LVGL 渲染，支持 `--width/--height`）
- `--backend pc`：PC 终端后端
- `--backend fb --fb-device /dev/fb0`：板卡 framebuffer 后端

示例：
```bash
# Linux: server/client 方式
./LinTx --server &
./LinTx -- system_state_mock --hz 5 &
./LinTx -- ui_demo --backend sdl --width 800 --height 480 --fps 30

# 板卡场景（示例）
./LinTx --server &
./LinTx -- system_state_mock --hz 5 &
LINTX_FB_ROTATE=270 LINTX_FB_SWAP_RB=1 ./LinTx -- ui_demo --backend fb --fb-device /dev/fb0 --width 800 --height 480
```

WSL2 测试建议：
```bash
cargo run --features sdl_ui --target x86_64-unknown-linux-gnu -- --server
# detach: 客户端退出后，后台模块继续运行（避免 UI 退出导致后台任务结束）
cargo run --features sdl_ui --target x86_64-unknown-linux-gnu -- --detach -- system_state_mock --hz 5
cargo run --features sdl_ui --target x86_64-unknown-linux-gnu -- -- ui_demo --backend sdl --width 800 --height 480 --fps 30
```

键盘操作（Launcher）：
- 主页面（第1页）是 `1x4` 横排应用：仅 `←/→` 有效
- 后续页面可扩展为 `2x4`：`↑/↓` 只在当前列移动，不跨列、不环绕
- `←/→` 在边界时切换前后页面
- `Enter` 进入应用页，`Esc` 返回，`Q` 退出

应用页交互（已实现）：
- `SYSTEM`：`↑/↓` 调整背光，`←/→` 调整音量
- `CONTROL`：实时查看 `adc_raw` 与 `mixer_out` 链路数据
- `MODELS`：`↑/↓` 选择机型，`Enter` 应用当前机型
- `CLOUD`：`Enter` 切换在线/离线并显示同步状态

## LVGL 架构设计（已接入真实 LVGL）
当前 `src/ui/` 分层与 `rpos` 架构融合方式如下：

- `ui/backend.rs`
  - `LvglBackend` trait：统一 PC 与 fb 后端接口
  - `BackendKind::PcApi | PcSdl | Fbdev`
  - `PcSdl` 路径：`lvgl::Display::register` + LVGL 控件树 + `tick_inc/task_handler`
- `ui/catalog.rs`
  - 应用模板定义：`AppSpec`（标题、图标文本、主题色）
  - 页面模板定义：`PageSpec`（行列布局、应用列表）
- `ui/model.rs`
  - `UiFrame`：统一 UI 数据模型（状态栏、launcher分页、选框位置）
- `ui/app.rs`
  - 主循环：订阅消息、按键事件、相对移动规则、应用进入/返回
- `messages.rs`
  - 统一消息定义：`adc_raw`、`system_status`、`system_config`

后续扩展建议：
- SG2002 板卡：在 `Fbdev` 后端接 `/dev/fb0`，显示驱动走 MIPI/fb
- 输入设备：将 SDL 键盘映射迁移到 LVGL indev 驱动，统一输入抽象
- 业务模块持续通过 `rpos::msg` 推送状态和配置，UI 只消费消息，不直接耦合驱动

## WSL / UI 排障
- 现象：`ui_demo --backend sdl` 启动后白屏  
  处理：先完整重启 server 与 UI 相关进程，再启动三段流程（server -> system_state_mock -> ui_demo）。
- 现象：UI 页面出现滚动条  
  处理：请使用当前版本（应用页容器已默认关闭滚动与滚动条）。
- 现象：文字显示方框  
  处理：当前 UI 文案已默认使用 ASCII，避免字形缺失；如仍有方框，请确认 SDL/LVGL 字体配置一致。

建议重启命令：
```bash
pkill -f "LinTx.*--server" || true
pkill -f "LinTx.*ui_demo" || true
pkill -f "LinTx.*system_state_mock" || true
```

## 许可证
本项目遵循 `MIT` 许可证（详见 `LICENSE` 文件）。

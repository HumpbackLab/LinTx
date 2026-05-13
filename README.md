# LinTx

## 项目简介
LinTx 是一个基于 Rust 的模块化遥控系统应用，当前主要面向两类运行环境：

- SG2002 板端：`riscv64gc-unknown-linux-musl`
- 桌面开发环境：Linux / Windows

当前仓库聚焦的不是飞控本体，而是遥控器侧的人机输入、混控、安全约束、ELRS/CRSF 链路和本地 UI。

## 当前定位
LinTx 遥控器端负责：

- 采样物理输入
- 做本地校准与混控
- 施加基础安全约束
- 输出 ELRS/CRSF 通道
- 提供本地配置与状态 UI

LinTx 不直接实现飞控侧模式逻辑。Angle、Horizon、Turtle、GPS Rescue / RTL 等模式，仍需在 Betaflight、ArduPilot、INAV 或 PX4 端把 AUX 通道范围绑定到对应功能。

## 默认按键与通道语义
默认模型的通道语义如下：

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

安全默认值：

- 没有明确开关输入时，`CH5 ARM`、`CH8 Turtle`、`CH10 GPS Rescue / RTL` 均保持 `0`
- 油门不低时，mixer 会强制 `CH5=0`

## 构建
常用构建命令：

```bash
cargo check
cargo check --features sdl_ui
cargo check --features lua
cargo check --features joydev_input
cross build --target riscv64gc-unknown-linux-musl --release --features lvgl_ui
```

说明：

- `lvgl_ui`：板端 LVGL / framebuffer 图形界面
- `sdl_ui`：桌面 SDL 窗口后端
- `lua`：Lua 脚本扩展
- `joydev_input`：Linux `joydev` 输入支持

板端 GUI 构建产物位于：

`target/riscv64gc-unknown-linux-musl/release/LinTx`

## 当前能力

### 输入链
当前仓库已覆盖这些输入来源：

- STM32 串口输入
- CRSF 遥控输入
- Mock 输入源
- Linux `joydev` 输入
- RC 按键转 UI 事件

其中 STM32 串口链路是当前 TX 方案的主路径：`STM32 采 ADC -> Linux 读串口 -> mixer -> RF 链路/UI`。

### 混控与配置
当前已具备：

- 摇杆校准与本地配置加载
- 通道混控
- 基于模型的 AUX 映射
- 基础安全约束

相关配置文件包括：

- `radio.toml`
- `joystick.toml`
- `mock_config.toml`
- `models/`

### RF / ELRS
当前 RF 侧以 `rf_link_service` 为主入口，兼容保留 `elrs_tx` 名称。当前实现重点包括：

- 持续发送 RC 通道
- ELRS 参数发现与状态同步
- UI 里的 ELRS 参数浏览与修改
- Bind / WiFi / 发射功率等交互
- 离线时的本地配置回退模式

README 不再展开命令行启动细节；如果要看实际板端工作流，直接以仓库里的板端脚本和源码为准。

### USB Gamepad
当前已支持把混控结果输出成 USB HID 手柄，并提供受控状态层：

- USB Gadget / HID 设备接入
- 运行态 ON / OFF 切换
- UI 中查看 HID 是否就绪
- UI 中触发输出开关

### UI
当前 UI 以 LVGL 为核心，桌面与板端共享同一套应用层状态模型。

当前 launcher 中的应用包括：

- `SYSTEM`
- `CONTROL`
- `MODELS`
- `CLOUD`
- `USB PAD`
- `AUX MAP`
- `ELRS`
- `ABOUT`

其中：

- `SYSTEM`：系统状态与基础配置
- `CONTROL`：输入链路与 mixer 输出观测
- `MODELS`：模型切换
- `CLOUD`：云状态占位页
- `USB PAD`：USB HID 手柄控制与状态
- `AUX MAP`：AUX 通道映射
- `ELRS`：ELRS 参数浏览、调整与反馈
- `ABOUT`：版本与项目信息

## UI 与运行时结构
当前 `src/ui/` 与运行时的主要分层如下：

- `src/ui/app.rs`：UI 主循环与页面切换
- `src/ui/model.rs`：统一 UI 状态模型
- `src/ui/apps/`：各应用页逻辑
- `src/ui/backend/`：terminal / SDL / fbdev 后端
- `src/ui/input/`：键盘、FIFO、事件输入

运行时基础由本仓库内的 `rpos/` 提供，负责：

- 模块注册
- 消息通道
- Unix socket client/server 运行方式

## 仓库结构
- `src/`：主程序与功能模块
- `rpos/`：本地运行时与消息基础设施
- `scripts/board/`：板端部署、验证与辅助脚本
- `docs/`：设计说明、验证记录与专题文档
- `third_party/`：本地 vendored 依赖

## 当前状态说明
README 只维护高层说明，不再作为逐模块启动手册。涉及这些内容时，应优先以源码和 `scripts/board/` 中的当前实现为准：

- 板端启动链路
- 验证脚本
- 临时调试流程
- 某个模块的完整参数细节

## 许可证
本项目遵循 `MIT` 许可证，详见 `LICENSE`。

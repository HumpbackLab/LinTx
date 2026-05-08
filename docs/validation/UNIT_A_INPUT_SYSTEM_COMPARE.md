# 功能单元 A 输入系统对照

本文件沿用 `docs/validation/UNIT_A_INPUT_SYSTEM.md` 里定义的输入验证范围，梳理当前可用的 LinTx 输入接口及其与 EdgeTX 参考实现的对应关系，并标注已有验证手段。

## 当前输入接口

1. `adc`（`src/input/adc.rs`）
   - 直接从 ADS1115（或板载 ADC）读取摇杆电压，刻度在 `ADCValue::channel()` 中被统一映射。
   - 作为主输入链的一部分，和 `joystick.toml` 校准文件配合用于 LinTx 的模拟摇杆物理量化。
2. `stm32_serial`（`src/input/stm32.rs`）
   - 从 STM32 侧的定制 UART 协议（帧头 `0x5A`）接收经过 ADC 采样一致化后的通道值，插入 mixer 前链路。
   - 当前方案把 STM32 视为主输入采样源，优先于 `adc`。
3. `crsf_rc_in`（`src/input/crsf_rc.rs`）
   - 作为兼容链路，将 CRSF/ELRS 所兼容的串口输入（形如 `CRSF RC` 往上传输）接入统一输入帧，使外部遥控器也能直接驱动 LinTx。
4. `joy_dev`（`src/input/joydev.rs`，需开启 `joydev_input` feature）
   - 利用 Linux `/dev/input/js*` 抽象读取 USB 手柄/摇杆，将其映射为 `InputFrame`，便于桌面调试或 trainer 模式。
5. `mock_joystick`（`src/input/mock.rs`）
   - 模拟输入源（`static`/`sine`/`step`），为验证输入链与 mixer/GUI 提供 deterministic 数据。
6. `rf_link_service`（`src/output/elrs/tx.rs`）
   - 发射侧从 `mixer_out` 生成 CRSF `RcChannels` 发往 ELRS 模块。
   - 同串口回传解析：可读取 CRSF 回传帧并更新基础 `system_status`，并发布输入侧 `elrs_feedback`（连接/信号/机载电池）。

## 对应 EdgeTX 参考

| LinTx 接口 | EdgeTX 参考 | 说明 |
| --- | --- | --- |
| `adc` | EdgeTX HAL ADC + `radio_diaganas.cpp` | EdgeTX 通过 `hal/adc` 和 `radio_diaganas` 直接读取四轴摇杆的模拟电压；LinTx `adc` 模拟同样的物理 ADC 视图，并输出标准化信号给 mixer。|
| `stm32_serial` | EdgeTX 的串口 + RC 输入处理（例如 `serial.c` 中的 `serialRead()`/`serialReadData()`） | EdgeTX 把 UART 串口当成多协议输入（SBUS/CRSF）；LinTx 把 STM32 串口前置成预处理后的 `InputFrame`，等价于 EdgeTX 读取来自某个 UART 的遥控输出来替代裸 ADC。|
| `crsf_rc_in` | EdgeTX 的 CRSF/XSR 接收器 (`crossfire.cpp`、`tasks.cpp` 中周期任务) | EdgeTX 可以通过 CRSF 接收器获取通道，LinTx 的 `crsf_rc_in` 也直接解析 CRSF 帧并将前 N 通道注入输入链，相当于 EdgeTX 的接收器输入逻辑。|
| `joy_dev` | EdgeTX 的 USB 手柄 + trainer 输入（`trainer.c`/`serial.c`） | EdgeTX 允许通过 trainer 端口或 USB 连接模拟摇杆；LinTx 的 `joy_dev` 提供类似的外部 joystick 源，适用于 PC 调试或 trainer 映射。|
| `mock_joystick` | EdgeTX 的模拟输入（例如 `simulator.cpp` 测试桩） | EdgeTX 在调试时也会注入模拟数据；LinTx 的 mock 模块专门用于验证 `InputFrame -> mixer` 的数据流，代表 EdgeTX 的内置模拟手段。|
| `rf_link_service`（兼容命令 `elrs_tx`） | EdgeTX 的 `pulses/crossfire.cpp` + `telemetry/crossfire.cpp` | EdgeTX 在同一 CRSF 物理链路上同时做通道发射与回传解析；LinTx 现在也在统一服务中实现了单串口收发并行的最小闭环。|

## 验证方法与现状

| 验证场景 | 涉及接口 | 目的 | 如何运行 | 备注 |
| --- | --- | --- | --- | --- |
| Mock 输入链 + GUI | `mock_joystick`, mixer, system_state_mock, `ui_demo --backend fb/sdl` | 确保统一输入帧到 UI 的展示（源、状态、通道值） | `scripts/board/tests/test_input_mock.sh`：启动 server + mock + mixer + UI | 验证 A-01、A-08、A-09 |
| `adc` 真实采样 | `adc` + `ui_demo` | 检查模拟摇杆可见 | `./LinTx --server & ./LinTx -- adc & ./LinTx -- ui_demo --backend fb` | 对应 A-02 |
| STM32 串口 | `stm32_serial` + mixer + UI | 确认 STM32 回传值在 Control 页面更新 | `scripts/board/tests/test_input_stm32.sh /dev/ttyS0 115200` 或手工链启动 | 对应 A-03；2026-04-01 在 `10.85.35.1` 上实测 `ttyS0` 有效 |
| CRSF 输入 | `crsf_rc_in` | 验证外部 RC 兼容输入 | `./LinTx --server & ./LinTx -- crsf_rc_in /dev/ttyS3 --baudrate 420000 & ./LinTx -- ui_demo --backend fb` | 对应 A-04，兼容 EdgeTX 的 CRSF 收发场景 |
| ELRS 回传状态 | `rf_link_service` + `mixer` + 主输入源 | 验证同串口收发并行、基础回传状态可见 | `sh ./scripts/board/tests/test_input_elrs_return.sh /dev/ttyS0 115200 /dev/ttyS3 115200` | 对应 A-11~A-14；输入源仍应保持为主输入，不应被回传链覆盖 |
| joydev 输入 | `joy_dev` | 检查 Linux 手柄/模拟器可见 | `./LinTx -- joy_dev /dev/input/js0` 配合 UI | 对应 A-05 |
| 错误路径 | `stm32_serial/crsf_rc_in/joy_dev` | 状态从 `Running` 跳 `Error` 并带 detail | 用不存在的设备路径启动 | 对应 A-06、A-10 |

## 目前能力边界与影响

- 输入链已统一为标准 `InputFrame`，EdgeTX 里对应的 `radio_diaganas`/`mixes`/`tasks` 接口仍可以通过配置文件自定义输入源和校准；LinTx 目前通过 `joystick.toml` 追踪校准，UI Control 页面也能实时显示 `channels` 与 `mixer_out`。
- EdgeTX 的许多输入能力（模拟 ADC、UART 串口、trainer/模拟器）在 LinTx 都有映射，只是在协议拆分上更偏向“模块 + 虚拟 frame”的方式，便于桌面/板卡共用基础链路。
- 当前验证主要依赖 `scripts/board/*` 脚本完成端到端链路，对应文档中列出的 A-01~A-10，尚未引入新测试用例。
- 当前 `rf_link_service` 已完成最小收口（兼容命令 `elrs_tx`），`elrs_agent` 已清理；系统层面仍未完成 TODO.md 里“参数树可操作”的目标。

## 与 TODO.md 的差距（单元 A 相关）

参考 [TODO.md](/home/shimmer/LinTx/LinTx_musl/TODO.md) 中“功能单元 A/E/F”，当前仍有以下缺口：

1. 结构缺口：`rf_link_service` 已收口，但 `input_service` 尚未落地。
当前输入源仍由多个模块并行发布，尚未形成统一输入服务边界。

2. 服务能力缺口：虽已消除串口竞争并完成 `rf_link_service`，但 ELRS 参数树读写/Bind 流程尚未打通。
这与 TODO 单元 E 的“GUI/CLI 可完成连接、浏览参数、修改参数、对频”仍不一致。

3. 回传覆盖缺口：已接入链路质量和机载电池，但未完整覆盖 EdgeTX 常见链路字段。

4. 验证流程缺口：已新增板测脚本覆盖回传路径，但尚未纳入自动化验证流程。
当前仍主要依赖人工执行脚本并观察 UI。

## 缺失的 EdgeTX 来源

EdgeTX 的输入源配置比单纯的物理摇杆更丰富：可选 Source 包含逻辑开关（`LS01~LS64`）、trainer 输入（`TR01~TR16`）、Channel 输出、全局变量（`GV01~GV09`）、定时器（`Tmr1~Tmr3`）、遥测数据（`Tele1~Tele64` 及其极值）、常量（`MAX`/`MIN`）、curves/servo 等抽象对象，甚至 Lua 脚本和 Special Function 也可以提供来源。这些来源不仅能直接送入 mixer，还可以作为 weight、offset、multiplier 的参考，对应 EdgeTX 的 UI、Lua 以及混控层的深度集成。

LinTx 当前只把五类实际硬件/模拟输入（`adc`、`stm32_serial`、`crsf_rc_in`、`joy_dev`、`mock_joystick`）统一接入 `InputFrame`，还没有把逻辑开关、遥测、定时器、trainer/脚本输出等抽象成可供 mixer 选用的“输入源”。因此，在对照 EdgeTX 的 Source List（详见 docs/EDGETX_PARITY_CHECKLIST.md 中提到的 `input_edit`/`curveedit` 结构）时，这些虚拟来源是目前尚未支持的能力方向。

| EdgeTX 来源类别 | 说明 | LinTx 状态 |
| --- | --- | --- |
| `LS01~LS64`（逻辑开关） | 用于基于开关条件触发 mix/输出/显示 | EdgeTX UI/逻辑层广泛引用；LinTx 尚未把逻辑开关输出当成 mixer 源。 |
| `TR01~TR16`（trainer 输入） | Trainer/UART 端口输入 | LinTx 有 `joy_dev` 作为桌面 trainer 替代但未直接支持 `TR` 源体系。 |
| `GV01~GV09`（全局变量） | 可编程变量、脚本/逻辑驱动 | EdgeTX 把 `GV` 作为混控 weight/offset；LinTx 目前无对应全局变量体系。 |
| `Tmr1~Tmr3`（定时器） | 定时器状态（启停/计时） | EdgeTX 可以把 timer 用作条件；LinTx 还未引入 timer 输入。 |
| `Tele1~Tele64`（遥测/极值） | 接收的 RSSI/电池等遥测值 | LinTx 目前只有 `system_state_mock`，真实遥测尚未打通，更别说作为输入。 |
| `MAX/MIN/Channel/Curve/Servo` 常量 | 常值、曲线/伺服限制 | EdgeTX mix 表可直接引用；LinTx mixer 目前仅读模型 output 配置。 |
| Lua/Special Function 输出 | 脚本或特殊功能的输出值 | EdgeTX Lua 可产出任意数据；LinTx 尚未有脚本输出回 input 的链路。 |

## 结论

LinTx 已把 ADC/STM32/CRSF/joydev/mock 五种输入源接入统一输入链，并在 Control 页面与 mixer 输出处可见，基本覆盖 EdgeTX 在模拟、串口、trainer、模拟器等物理输入能力。当前 `rf_link_service` 已实现“发射 + 回传状态 + 输入反馈”的最小闭环，并新增板级脚本固定回传验证路径；下一步应优先补齐 ELRS 参数树可操作能力与遥测字段语义。

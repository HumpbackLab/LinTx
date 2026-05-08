# 功能单元 A 验证

## 范围

功能单元 A 的目标是把所有输入源统一接入标准输入链，并让 UI/CLI 能看到：

- 当前输入源
- 输入源状态
- 通道值
- 通道数量

当前覆盖输入源：

- `adc`
- `stm32_serial`
- `crsf_rc_in`
- `joy_dev`
- `mock_joystick`

说明：

- 对当前 TX 设备，主输入链应优先理解为 `adc` 或 `stm32_serial`
- `crsf_rc_in` 是兼容型输入源，用来接外部 CRSF 数据源，不是这台 TX 的典型板级主路径
- `rf_link_service` 负责 `mixer_out -> CRSF RC` 发射，同时在同串口解析 ELRS/CRSF 回传状态（链路质量、电池等），该能力属于输入相关状态链路，不替代主输入源
- 兼容说明：当前仍保留 `elrs_tx` 命令别名，底层实现已收口到 `rf_link_service`

## 板级验证前提

### 1. 构建

主机侧：

```bash
cargo check
cross build --target riscv64gc-unknown-linux-musl --release --features lvgl_ui
```

### 2. 部署

按现有板端部署脚本把二进制和脚本同步到板子，例如：

```bash
sh scripts/board/deploy_to_board.sh
```

### 3. 板端基础环境

- 可启动 `LinTx --server`
- 板端存在 `/dev/fb0`
- 如果要测真实串口输入，确认 `/dev/ttyS*` 存在
- 如果要测 `joy_dev`，确认 `/dev/input/js*` 存在
- 默认 UI 参数来自 [scripts/board/lib/board_common.sh](/home/shimmer/LinTx/LinTx_musl/scripts/board/lib/board_common.sh)

## 建议板级验证方法

### A. 先验证 UI 展示链是否正常

无真实输入时，应使用专门的输入验证脚本（如 `test_input_mock.sh`），不要使用旧的 ELRS mock GUI 脚本。
说明：`elrs_agent` 与相关 `test_gui_*` 脚本已清理，不再作为验证入口。

建议执行：

```bash
cd /root/lintx
sh ./scripts/board/tests/test_input_mock.sh
```

进入 Control 页面，检查：

- 输入源显示为 `Mock`
- 状态显示为 `Running`
- 可看到通道数量
- 可看到 CH1 到 CH4 的值变化
- Mixer Out 仍然有输出

### B. 再验证真实串口输入链

如果要验证外部 CRSF 输入兼容源，可执行：

```bash
cd /root/lintx
./LinTx --server &
./LinTx -- crsf_rc_in /dev/ttyS3 --baudrate 420000 &
LINTX_FB_ROTATE=270 LINTX_FB_SWAP_RB=1 ./LinTx -- ui_demo --backend fb --fb-device /dev/fb0 --width 800 --height 480
```

但对当前 TX 设备，更推荐验证 STM32 主输入链：

```bash
cd /root/lintx
sh ./scripts/board/tests/test_input_stm32.sh /dev/ttyS0 115200
```

或手工执行：

```bash
./LinTx --server &
./LinTx -- stm32_serial /dev/ttyS0 --baudrate 115200 &
./LinTx -- mixer &
LINTX_FB_ROTATE=270 LINTX_FB_SWAP_RB=1 ./LinTx -- ui_demo --backend fb --fb-device /dev/fb0 --width 800 --height 480
```

实测记录：

- 2026-04-01 在 `10.85.35.1` 上，`/dev/ttyS0 @ 115200` 可稳定读到 STM32 帧
- 同板上 `/dev/ttyS3` 未读到有效 STM32 数据
- 实际帧格式与当前 Python 验证脚本一致：`0x5A + len(12) + type(0x01) + 4x u16 + reserve(u16) + crc8_dvb_s2`

检查：

- 当前输入源名称正确
- 状态为 `Running`
- 通道值随遥杆动作变化
- 通道值刷新稳定，没有卡死或明显跳变异常

### B2. 验证 RF 链路回传状态链（新增）

目标：验证当前 `rf_link_service` 已接入“单串口收发”路径，可在发送 RC 的同时解析回传并更新 `system_status` 与输入侧反馈状态。

建议执行（板端，推荐脚本）：

```bash
cd /root/lintx
sh ./scripts/board/tests/test_input_elrs_return.sh /dev/ttyS0 115200 /dev/ttyS3 115200
```

检查：

- Control 页面输入源仍是 `STM32 Serial`（或当前主输入源），不应变成 `CRSF RC In`
- Control 页面可见 ELRS 反馈行（连接状态/Signal/Aircraft Battery）
- Telemetry/System 区域中 `Signal` 会随链路变化
- 若接收机有 CRSF `BATTERY_ID` 回传，`Aircraft Battery` 应有更新
- `Mixer Out` 和发射链持续工作，不因回传解析卡住
- 建议保持主输入在线（`stm32_serial` 或 `mock_joystick`），确保 `rf_link_service` 有持续 RC 通道流

日志建议：

- 给 `rf_link_service` 单独记录日志（如 `/tmp/lintx-elrs/rf_link_service.log`）
- 若有串口抓包，同时记录 `ttyS3` 原始字节流，便于核对 `LINK_ID/LINK_RX_ID/BATTERY_ID`

### C. 验证 mixer 消费的已是统一输入帧

板端同时启动 `mixer`：

```bash
./LinTx --server &
./LinTx -- mock_joystick &
./LinTx -- mixer &
LINTX_FB_ROTATE=270 LINTX_FB_SWAP_RB=1 ./LinTx -- ui_demo --backend fb --fb-device /dev/fb0 --width 800 --height 480
```

检查：

- Control 页面存在输入通道显示
- Mixer Out 跟随输入变化
- 运行链在不关注 `adc_raw` 语义的情况下仍可正常工作

### D. 验证错误状态可见

对不存在的设备执行：

```bash
./LinTx -- joy_dev /dev/input/js999
```

或：

```bash
./LinTx -- crsf_rc_in /dev/ttyS99 --baudrate 420000
```

检查：

- 输入状态进入 `Error`
- detail 中包含打开设备失败信息

## 功能清单

| 编号 | 功能点 | 检查方法 | 预期结果 | 实际结果 | 结论 |
|---|---|---|---|---|---|
| A-01 | mock 输入源接入统一输入链 | 运行 `test_input_mock.sh`，查看 Control 页面 | Source=Mock，Status=Running，可见通道值 |  |  |
| A-02 | ADC 输入源标识正确 | 启动 `adc` + `ui_demo` | Source=ADC，Status=Running |  |  |
| A-03 | STM32 串口输入接入统一输入链 | 启动 `stm32_serial` + UI | Source=STM32 Serial，通道值随输入变化 |  |  |
| A-04 | 外部 CRSF 输入兼容源可用 | 启动 `crsf_rc_in` + UI | Source=CRSF RC In，至少前 4 通道映射正确 |  |  |
| A-05 | joydev 输入接入统一输入链 | 启动 `joy_dev /dev/input/jsX` + UI | Source=joydev，摇杆动作可见 |  |  |
| A-06 | 输入源状态可见 | 分别启动各输入模块 | 可见 Running 或 Error |  |  |
| A-07 | 通道数量可见 | 使用多通道输入源如 CRSF | UI 可显示 count，且不再限定 4 通道语义 |  |  |
| A-08 | Mixer 能消费统一输入帧 | 启动 `mock_joystick + mixer + UI` | Mixer Out 正常变化 |  |  |
| A-09 | UI Control 页面不再写死 ADC 语义 | 打开 Control 页面 | 展示 Input Source / Status / Channels |  |  |
| A-10 | 错误设备路径时状态可观测 | 启动不存在的设备 | 输入状态进入 Error，并带 detail |  |  |
| A-11 | ELRS 回传链路可工作（单串口收发） | 启动 `stm32_serial + mixer + rf_link_service + ui_demo` | 发射与回传可并行，UI 不冻结 |  |  |
| A-12 | 回传链路质量可见 | 在链路波动时观察 UI | Signal 值有变化 |  |  |
| A-13 | 回传电池可见（若接收机上报） | 接入带电池遥测的接收机 | Aircraft Battery 可变化 |  |  |
| A-14 | 主输入链语义不被回传链覆盖 | 同时运行主输入+rf_link_service | 输入源仍显示主输入（如 STM32） |  |  |

## 测试记录

### 执行记录 1

- 日期：
- 测试人：
- 板卡：
- 输入源：
- 命令：
- 日志路径：
- 结果：

### 执行记录 2

- 日期：
- 测试人：
- 板卡：
- 输入源：
- 命令：
- 日志路径：
- 结果：

## 日志建议

建议统一记录这些日志位置：

- `/tmp/lintx-elrs/server.log`
- `/tmp/lintx-elrs/ui.log`
- `/tmp/lintx-elrs/elrs_mock.log`
- `/tmp/lintx-elrs/elrs_crsf.log`
- `/tmp/lintx-elrs/input_mock.log`
- `/tmp/lintx-elrs/input_stm32.log`
- `/tmp/lintx-elrs/rf_link_service.log`
- `/tmp/lintx-elrs/elrs_tx.log`（仅命令别名兼容路径）

如果是手工启动，也建议补记：

- 模块 stdout/stderr 重定向路径
- 板端串口抓包路径
- UI 截图文件名

## 当前 host 侧通过项

这次代码修改后，本地已验证：

- `cargo check`
- `cargo test mixer::tests -- --nocapture`
- `cargo check --features joydev_input`

这些只能证明 host 编译和局部逻辑通过，不能替代板级验证。

## 当前未完成项与协作依赖（基于 TODO.md）

参考 [TODO.md](/home/shimmer/LinTx/LinTx_musl/TODO.md) 的功能单元 E/F，当前状态如下：

1. 当前已落地 `rf_link_service`（并保留 `elrs_tx` 别名），完成“单 UART、单服务”最小收口。
需要配合：把 ELRS 参数树真实读写接入 `elrs_cmd`，当前仍是最小状态回显，不等于参数配置能力完成。

2. 回传解析目前只覆盖最小状态字段（`LINK_ID`/`LINK_RX_ID`/`BATTERY_ID`）与输入侧 `elrs_feedback`。
需要配合：补齐 `RF Mode`、`TX Power`、时间戳/新鲜度、连接状态机，并统一状态消息结构。

3. UI 当前可显示基础状态，但“字段来源与缺失字段”提示不充分。
需要配合：在 UI/CLI 明确区分 `真实回传`、`默认值`、`无数据`，避免误判链路健康。

4. `remote_battery_percent` 尚无稳定真实来源接入。
需要配合：确认来源（CRSF 可用字段、模块参数、或本机电池采样）后再接入。

5. 板级验证脚本已新增 `test_input_elrs_return.sh` 覆盖 `rf_link_service` 回传路径。
需要配合：在 CI/自动化中纳入该脚本或等价步骤，避免只靠人工执行。

## 本次更新说明（2026-04-01）

- 完成项（可交付闭环）：
  - 串口收发已统一到 `rf_link_service`，并保留 `elrs_tx` 命令兼容入口
  - 输入监视链新增 ELRS 回传反馈显示（`elrs_feedback`）
  - 新增板级脚本 `scripts/board/tests/test_input_elrs_return.sh`
- 非目标（本次不做）：
  - 不实现完整 ELRS 参数树读写/目录导航/写入确认
  - 不补齐全部遥测字段语义
- 对应 TODO 单元 E 进度备注：
  - 已完成“单 UART、单服务”架构前提
  - 未完成“GUI/CLI 可完成参数浏览修改与 Bind”的能力闭环，需后续单元继续推进

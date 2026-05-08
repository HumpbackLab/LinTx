# LinTx 与 EdgeTX 对照开发清单

## 目标

`LinTx` 的目标不是复刻 EdgeTX 的板级 HAL，而是把 EdgeTX 的核心遥控系统能力迁移为 Linux/跨平台应用：

1. 统一输入抽象
2. 模型与混控系统
3. 协议输出与遥测
4. UI 与配置持久化
5. 后续可扩展脚本能力

当前 `LinTx` 已完成的底层链路是：

1. 输入采集
2. 基础校准
3. 基础归一化混控
4. 基础输出
5. 基础 UI 框架与消息总线

当前真正缺的是中层和上层系统，不是更多输入驱动。

## 当前仓库与参考位置

| 对象 | 位置 | 用途 |
| --- | --- | --- |
| LinTx 主仓库 | `/home/shimmer/LinTx/LinTx_musl` | 当前开发仓库 |
| EdgeTX 参考仓库 | `/home/shimmer/LinTx/EdgeTX_ref` | 只读参考 |
| LinTx 主入口 | `src/main.rs` | 模块调度、client/server |
| LinTx UI 入口 | `src/ui/runner.rs` | LVGL UI 入口 |
| EdgeTX 主循环 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/main.cpp` | `perMain()` |
| EdgeTX 主任务入口 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/tasks.cpp` | 周期任务与启动 |
| EdgeTX 主数据结构 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/datastructs_private.h` | `ModelData` / `RadioData` |

## LinTx 当前已实现能力

| 能力 | LinTx 位置 | 状态 | 备注 |
| --- | --- | --- | --- |
| 模块化运行时 | `src/main.rs`、`rpos/src/module.rs`、`rpos/src/msg.rs` | 已实现 | 模块注册、消息总线、Unix socket client/server |
| 输入采集 | `src/input/adc.rs`、`src/input/stm32.rs`、`src/input/crsf_rc.rs`、`src/input/mock.rs`、`src/input/joydev.rs` | 已实现 | 当前已统一到 `InputFrameMsg`；其中 `stm32_serial` 更符合 TX 主输入链，`crsf_rc_in` 属于外部 CRSF 输入兼容源 |
| 摇杆校准 | `src/input/calibrate.rs` | 已实现 | 生成 `joystick.toml` |
| 基础混控 | `src/control/mixer.rs` | 部分实现 | 仅 4 通道线性归一化 |
| CRSF/ELRS 发射链 | `src/output/elrs/tx.rs` | 部分实现 | `mixer_out -> CRSF RC -> 串口` |
| USB HID 输出 | `src/output/usb_gamepad/driver.rs` | 已实现 | 映射为 HID gamepad |
| 系统状态消息 | `src/messages.rs`、`src/tools/system_state_mock.rs` | 部分实现 | 主要还是 mock |
| UI 框架 | `src/ui/runner.rs`、`src/ui/app.rs`、`src/ui/backend/` | 部分实现 | launcher + diagnostics + 少量交互 |
| Windows 本地模式 | `src/main.rs` | 已实现 | 不依赖 Unix socket |

## EdgeTX 核心功能与最小阅读路径

后续开发不需要通读整个 `EdgeTX`。按能力只读下面这些关键文件即可。

### 1. 主循环与整机调度

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 如何组织整机周期任务 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/tasks.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/main.cpp` | `tasksStart()` 和 `perMain()` 是所有能力的总入口 |
| 看 LinTx 当前如何组织运行 | `src/main.rs` | `rpos/src/module.rs`、`rpos/src/msg.rs` | 对照当前是“模块驱动”，不是“整机循环” |

### 2. 输入、校准、硬件诊断

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 输入校准流程 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/radio/radio_calibration.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/radio/radio_diaganas.cpp` | 校准状态机和诊断页都在这里 |
| 看 LinTx 当前输入链 | `src/input/adc.rs`、`src/input/stm32.rs`、`src/input/crsf_rc.rs` | `src/input/calibrate.rs` | 对照当前只有基础采样和校准 |

### 3. 模型数据结构

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 的模型和电台配置长什么样 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/datastructs_private.h` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/datastructs.h` | `ModelData` / `RadioData` 是后续所有功能的根 |
| 看 LinTx 当前有哪些状态对象 | `src/messages.rs` | `src/ui/model.rs` | 现状是消息和 UI 状态分散，没有统一模型结构 |

### 4. 混控、Expo、曲线、输出限制

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 的 mix 主体 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/mixes.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/model_init.cpp` | 一个负责 mix line，一个负责默认模型初始化 |
| 看 EdgeTX 的输入编辑 UI 关联哪些概念 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/model/input_edit.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/model/curveedit.cpp` | 用来反推最小可用配置面 |
| 看 LinTx 当前 mixer | `src/control/mixer.rs` | `src/input/calibrate.rs` | 当前仅做校准后归一化 |

### 5. 逻辑开关、特殊功能、飞行模式

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 条件逻辑层 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/switches.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/datastructs_private.h` | `logicalSw`、`customFn`、`flightModeData` 都依赖统一模型结构 |
| LinTx 当前对应位置 | 无 | 无 | 这一层目前完全没有实现 |

### 6. 协议输出与模块抽象

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 如何抽象 RF/协议输出 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/pulses/pulses.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/pulses/` 目录下具体协议文件 | 先看总入口，再看具体协议 |
| 看 EdgeTX 模型如何持有模块配置 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/datastructs_private.h` | 无 | `moduleData` 是协议输出配置来源 |
| 看 LinTx 当前实现 | `src/output/elrs/tx.rs` | `src/output/usb_gamepad/driver.rs` | 目前没有统一“输出模块抽象” |

### 7. 遥测系统

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 遥测总入口 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/telemetry/telemetry.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/telemetry/telemetry_sensors.cpp` | 先理解调度，再理解传感器模型 |
| 看具体协议解析 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/telemetry/crossfire.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/telemetry/frsky*.cpp` 等 | 后续按需要只读某一协议 |
| 看 LinTx 当前状态 | `src/messages.rs` | `src/tools/system_state_mock.rs` | 当前还没有真实遥测协议栈 |

### 8. 模型存储与持久化

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 的存储总入口 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/storage.h` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/storage_common.cpp` | 先看接口，再看 dirty/writeback |
| 看模型列表和模型发现 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/modelslist.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/yaml/` | 按需看 YAML 序列化 |
| 看 LinTx 当前持久化 | `src/input/calibrate.rs` | `joystick.toml` | 当前只有校准文件 |

### 9. UI 与页面组织

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 主视图 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/mainview/view_main.cpp` | 同目录其他 `view_*.cpp` | 看飞行中主视图、通道页、统计页 |
| 看 EdgeTX 电台页 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/radio/` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/model/` | 前者偏 radio setup，后者偏 model setup |
| 看 LinTx UI 结构 | `src/ui/app.rs` | `src/ui/backend.rs`、`src/ui/model.rs`、`src/ui/catalog.rs` | 当前更多是 launcher 框架，不是完整 radio UI |

### 10. Lua / Widget / 主题

| 目的 | 先读文件 | 再读文件 | 为什么读 |
| --- | --- | --- | --- |
| 看 EdgeTX 脚本入口 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/lua/interface.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/lua/api_model.cpp`、`api_general.cpp` | 先看入口，再看暴露给脚本的对象 |
| 看 Widget 体系 | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/lua/widgets.cpp` | `/home/shimmer/LinTx/EdgeTX_ref/radio/src/lua/api_colorlcd_lvgl.cpp` | 如果未来要做扩展 UI，这两个是关键 |
| LinTx 当前对应位置 | 无 | 无 | 目前不建议优先实现 |

## 精简对照表

| 能力 | EdgeTX 参考点 | LinTx 当前状态 | 判断 |
| --- | --- | --- | --- |
| 整机周期主循环 | `tasks.cpp` + `main.cpp::perMain()` | 只有模块式运行 | 缺 |
| 统一模型结构 | `datastructs_private.h::ModelData` | 没有 | 缺 |
| 电台全局结构 | `datastructs_private.h::RadioData` | 只有零散消息 | 缺 |
| 输入采集 | `hal/*` + radio diag | 已有多路输入 | 有 |
| 输入校准 | `radio_calibration.cpp` | 有基础版 | 弱 |
| 混控/Expo/曲线/输出限制 | `mixes.cpp` + model UI | 只有线性归一化 | 缺 |
| 逻辑开关/特殊功能/飞行模式 | `switches.cpp` + `ModelData` | 没有 | 缺 |
| 协议输出抽象 | `pulses.cpp` + `moduleData` | 只有 `elrs_tx` 单链路 | 弱 |
| 遥测 | `telemetry.cpp` + sensors | 只有 mock status | 缺 |
| 模型持久化 | `storage_common.cpp` + modelslist | 只有 `joystick.toml` | 缺 |
| 完整 radio UI | `gui/colorlcd/*` | 只有 launcher/diagnostics | 弱 |
| Lua/widget | `lua/*` | 没有 | 缺 |

## TODO 顺序

下面的顺序按“依赖关系 + 价值密度 + 对后续工作的解锁程度”排序。

### P0：先补骨架，不补骨架后面都会返工

已完成：第 1 项，统一配置核心结构已落地到 `src/config/mod.rs`。

已完成：第 2 项，配置持久化与默认模型布局已落地到 `src/config/store.rs`，实际会生成 `radio.toml` 和 `models/*.toml`。

已完成：第 3 项，UI 的 `MODELS` 页已接到真实模型文件，并会切换当前活动模型。

1. [已完成] 定义 `LinTx` 的统一配置核心结构
   - 目标：引入 `RadioConfig` + `ModelConfig` 等价物
   - 先做内容：输入映射、通道定义、输出定义、协议配置、UI 需要的基础字段
   - 参考：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/datastructs_private.h`
   - LinTx 落点建议：新增 `src/config/` 或 `src/model/`

2. [已完成] 建立配置持久化与加载机制
   - 目标：不要再靠零散 `toml` 文件临时拼装状态
   - 先做内容：`radio.toml` + `models/*.toml` 或单文件 `models.toml`
   - 参考：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/storage.h`
   - LinTx 落点建议：`src/config/store.rs`

3. [已完成] 把 UI 的模型列表接到真实配置
   - 目标：把当前 `MODELS` 页从静态占位变成真实模型切换入口
   - 依赖：前两项完成后再做
   - LinTx 现状：`src/ui/app.rs`、`src/ui/backend.rs` 现在还是静态字符串

### P1：补中层核心能力，形成真正可配置的遥控链

4. 重构 `mixer` 为配置驱动的处理链
   - 目标：从“4 通道线性映射”升级为：`input -> expo -> curve -> mix -> output`
   - 最小先做：输入源选择、权重、reverse、subtrim、输出范围
   - 参考：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/mixes.cpp`
   - LinTx 落点建议：拆成 `src/mixer/` 目录，而不是单文件

5. 补输出模块抽象层
   - 目标：统一 `elrs_tx`、`usb_gamepad`，避免每种输出都直接绑死 `mixer_out`
   - 最小先做：定义 `OutputFrame` / `RfFrame` 抽象和模块配置
   - 参考：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/pulses/pulses.cpp`

6. 引入最小真实 telemetry 模型
   - 目标：不要再只有 `system_state_mock`
   - 最小先做：先支持 `CRSF/ELRS` 相关链路质量、电池、电压、RSSI/LQ
   - 参考：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/telemetry/telemetry.cpp`、`crossfire.cpp`
   - LinTx 落点建议：新增 `src/telemetry/`

### P2：补配置层表达能力，让它开始像 EdgeTX

7. 加入 flight mode / rate profile 的最小版本
   - 目标：先不全抄 EdgeTX，但要有“同一模型多套参数”的能力
   - 可先做 2 到 4 个 profile
   - 这一步会明显提升模型系统价值

8. 加入逻辑开关的最小子集
   - 目标：先支持最常用条件表达式，不要一开始全量抄
   - 最小建议：比较、阈值、edge、sticky 四类
   - 参考：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/switches.cpp`

9. 加入特殊功能的最小子集
   - 目标：让逻辑条件能够触发动作
   - 最小建议：切模型参数、切输出模式、播放提示、切 UI 页/状态

### P3：补 UI 使其真正可操作

10. 重构 UI 信息架构
   - 目标：从 launcher 演示变成 radio app
   - 先做页面：Model Select、Inputs、Mixer、Outputs、Telemetry、Radio Setup
   - 参考读取顺序：
     - `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/mainview/view_main.cpp`
     - `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/radio/`
     - `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/model/`

11. 把 UI 全部接真实后端数据
   - 目标：消灭静态模型名、假 cloud 状态、占位模板页
   - LinTx 现状：`src/ui/backend.rs` 里仍有 `Template placeholder` 和静态 model 列表

### P4：最后再考虑高级扩展

12. Trainer 系统
   - 没有前面模型/混控/协议抽象时，trainer 做了也容易返工

13. 音频提示系统
   - 先做事件总线后再接蜂鸣/语音更合理

14. Lua / widget 扩展
   - 这一步价值高，但必须放后面
   - 没有稳定数据模型、UI API、配置系统之前，上脚本层会把复杂度放大

## 后续开发时的最小阅读集

如果下一步开始实现，不建议每次都翻整个 `EdgeTX_ref`。按任务读下面这些集合就够了。

### 做模型系统时

1. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/datastructs_private.h`
2. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/storage.h`
3. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/storage_common.cpp`
4. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/modelslist.cpp`

### 做 mixer 时

1. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/mixes.cpp`
2. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/model_init.cpp`
3. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/model/input_edit.cpp`

### 做 telemetry 时

1. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/telemetry/telemetry.cpp`
2. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/telemetry/telemetry_sensors.cpp`
3. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/telemetry/crossfire.cpp`

### 做 UI 时

1. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/mainview/view_main.cpp`
2. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/radio/`
3. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/gui/colorlcd/model/`

### 做脚本时

1. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/lua/interface.cpp`
2. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/lua/api_model.cpp`
3. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/lua/widgets.cpp`

## 结论

后续开发顺序应当是：

1. 统一模型结构
2. 持久化
3. 配置化 mixer
4. 输出抽象
5. 真实 telemetry
6. flight mode / logical switch / special function
7. 真正的 radio UI
8. trainer / audio / lua

如果偏离这个顺序，最容易出现的问题是：UI 做得很多，但后端没有统一结构；或者协议做得很多，但模型系统缺位，最后整体返工。

## 当前可验证的模型导入与使用

现在已经可以验证“模型文件导入并被系统使用”这条链路。

### 已落地的验证闭环

1. 程序启动时会自动确保存在 `radio.toml` 和 `models/`
2. 当前仓库会自动生成示例模型：`models/quad_x.toml`、`models/fixed_wing.toml`、`models/rover.toml`
3. `ui_demo` 的 `MODELS` 页面会显示 `models/` 里的真实模型，而不是静态占位字符串
4. 在 `MODELS` 页面按 `Enter` 选择模型后，会更新 `radio.toml` 中的 `active_model`
5. `mixer` 会读取活动模型，并把模型里的 `weight`、`offset`、`reversed`、`limits` 应用到输出

### 你现在可以怎么验证

1. 查看磁盘模型文件
   - `radio.toml`
   - `models/quad_x.toml`
   - `models/fixed_wing.toml`
   - `models/rover.toml`

2. 启动一条最小验证链路
   - `cargo run -- --server`
   - `cargo run -- --detach -- system_state_mock --hz 5`
   - `cargo run -- --detach -- mock_joystick`
   - `cargo run -- --detach -- mixer`
   - `cargo run --features sdl_ui -- -- ui_demo --backend sdl --width 800 --height 480 --fps 30`

3. 在 UI 里验证模型导入
   - 进入 `MODELS`
   - 你会看到来自 `models/` 的真实模型名和协议
   - 选中某个模型，按 `Enter`

4. 在文件上验证模型切换
   - 查看 `radio.toml` 的 `active_model` 是否变化

5. 在功能上验证模型使用
   - 切换到 `CONTROL`
   - 观察 `mixer_out`
   - 在 `quad_x`、`fixed_wing`、`rover` 之间切换时，输出会因 `weight` / `offset` / `reversed` 不同而变化

### 导入你自己的模型

最简单的方式就是直接往 `models/` 放一个新的 `.toml` 文件。

建议做法：

1. 复制 `models/quad_x.toml` 为新文件，比如 `models/my_plane.toml`
2. 修改 `id`、`name`、`output.protocol`
3. 修改 `mixer.outputs` 中各通道的 `weight`、`offset`、`limits.reversed`
4. 重新打开 `ui_demo`，新模型就会出现在 `MODELS` 页面

### 当前边界

当前“模型使用”已经接通，但仍然是最小版本：

1. `mixer` 目前只使用了模型里的输出侧参数，没有完整实现 `input -> expo -> curve -> mix -> output`
2. 还没有模型导入/导出的专门 CLI
3. 还没有 flight mode、logical switch、special function

## EdgeTX 兼容策略

目标调整为：

1. 体验与语义尽量兼容 `EdgeTX`
2. `LinTx` 内部格式继续保持 Rust 友好的 `toml`
3. 通过导入/导出或转换脚本兼容 `EdgeTX` 的交换格式

### 结论

不建议追求下面两种兼容：

1. `EdgeTX` 内部 EEPROM / 二进制布局兼容
2. `EdgeTX` 内部内存结构一比一复刻

建议追求下面这种兼容：

1. `LinTx` 内部格式独立
2. 支持 `EdgeTX YAML <-> LinTx TOML` 的导入导出
3. 如果导出完全兼容暂时做不到，先提供稳定转换脚本

### 为什么选 YAML 交换层

从 `EdgeTX` 代码看，YAML 已经是明确的存储/交换入口之一：

1. 入口文件：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/sdcard_yaml.cpp`
2. YAML 解析器：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/yaml/yaml_parser.h`
3. YAML 数据结构入口：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/yaml/yaml_datastructs.h`
4. 具体机型布局：`/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/yaml/yaml_datastructs_*.cpp`

这说明：

1. `EdgeTX` 并不是只认内部二进制布局
2. YAML 已经是它自己的正式结构化表示层之一
3. `LinTx` 最合理的兼容点就是这里

### 关键字段的交换入口

从 `struct_ModelData` 的 YAML 节点看，后续导入导出最值得先支持的是这些字段：

1. `header`
2. `timers`
3. `mixData`
4. `limitData`
5. `expoData`
6. `curves`
7. `points`
8. `logicalSw`
9. `customFn`
10. `flightModeData`
11. `moduleData`
12. `failsafeChannels`
13. `trainerData`
14. `telemetrySensors`

参考位置：

1. `/home/shimmer/LinTx/EdgeTX_ref/radio/src/storage/yaml/yaml_datastructs_tx16smk3.cpp`
2. 其他机型的 `yaml_datastructs_*.cpp`

### 建议的兼容实现方式

采用三层结构：

1. `LinTx` 内部模型：`RadioConfig` / `ModelConfig`
2. `EdgeTX` 交换模型：`EdgeTxRadioYaml` / `EdgeTxModelYaml`
3. 转换层：`from_edgetx_yaml()` / `to_edgetx_yaml()`

建议不要让 `LinTx` 运行时直接依赖 `EdgeTX` YAML 原样结构；中间一定要有转换层。

### 兼容范围建议

第一阶段先支持：

1. 模型名和基本标识
2. 通道输入映射
3. `mixData` 的最小子集
4. `limitData` 的最小子集
5. `expoData` 和曲线最小子集
6. `moduleData` 的协议类型最小子集
7. `failsafeChannels`
8. `flightModeData` 的最小子集

第二阶段再支持：

1. `logicalSw`
2. `customFn`
3. `telemetrySensors`
4. `trainerData`

最后再考虑：

1. `scriptsData`
2. widget / theme 相关
3. `screenData` / `topbarData`
4. 更深的 radio-level 配置

### 对当前 LinTx 的要求

因此，从现在开始，`LinTx` 的配置设计要遵守一个新约束：

1. 内部字段命名可以自由
2. 但语义边界要尽量靠近 `EdgeTX`
3. 未来要能稳定映射到 `mixData` / `limitData` / `expoData` / `moduleData` 这些概念

这意味着后续在做 `mixer`、`output`、`telemetry` 时，应该优先对齐语义，而不是优先对齐文件格式。

## 兼容开发顺序

在原来的开发顺序里，加入 `EdgeTX` 交换兼容后的建议顺序如下：

1. 完成 `LinTx` 内部模型与持久化
2. 完成配置驱动的 `mixer`
3. 定义 `EdgeTX YAML` 的字段映射表
4. 实现 `EdgeTX YAML -> LinTx TOML` 导入器
5. 实现 `LinTx TOML -> EdgeTX YAML` 导出器或转换脚本
6. 再补 `logical switch` / `special function` / `telemetry`
7. 再做更完整的 UI 编辑器

原因很简单：

1. 没有稳定内部模型，就没法做稳定导入器
2. 没有配置驱动的 `mixer`，导入 `mixData` 也没有意义
3. 先做导入，能最快验证兼容路线是否成立

## 下一步建议

如果继续做，我建议下一项不是直接补 UI，而是：

1. 把 P1 第 4 项 `mixer` 配置链补完整
2. 紧接着新增一份 `docs/EDGETX_FIELD_MAPPING.md`
3. 在那份文档里列出 `LinTx ModelConfig` 对 `EdgeTX ModelData YAML` 的字段映射
4. 然后实现一个最小 `EdgeTX YAML -> LinTx TOML` 转换器

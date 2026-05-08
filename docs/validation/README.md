# LinTx Validation

这个目录专门存放功能单元验证方案、板级测试步骤和实际测试结果。

建议约定：

- 每个功能单元单独一个文件，例如 `UNIT_A_INPUT_SYSTEM.md`
- 先写测试目标、环境、步骤、检查项
- 每次执行后把结果、日志位置、问题记录追加到同一文件
- 如果后续需要整机联调，可以再新增 `E2E_*.md`

当前文件：

- `TEMPLATE.md`：新功能单元验证模板
- `UNIT_A_INPUT_SYSTEM.md`：功能单元 A 输入系统板级验证清单
- `UNIT_A_INPUT_SYSTEM_COMPARE.md`：功能单元 A 与 EdgeTX 对照、差距与后续协作项

板端输入验证常用脚本：

- `scripts/board/tests/test_input_mock.sh`
- `scripts/board/tests/test_input_stm32.sh`
- `scripts/board/tests/test_input_elrs_return.sh`

推荐记录格式：

- 日期
- 测试人
- 板卡/固件版本
- 命令
- 预期结果
- 实际结果
- 日志路径
- 结论
- 遗留问题

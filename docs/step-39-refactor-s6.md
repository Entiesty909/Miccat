# Step 39 — S6 清理与门禁状态

范围：
- 做了：拆分 `config/mod.rs`、`config/store.rs`、`config/single_instance.rs`。
- 做了：删除未接线的声板 Pad/播放器配置、热键模式/停止热键、自动重连字段和未接线播放按键设置及契约。
- 做了：删除已核实未调用的音频转换、设备、试听、SFX 和引擎 API；保留真实运行路径。
- 做了：将 `scripts/check.cmd` 切换为 `clippy -- -D warnings` 严格门禁。
- 做了：保留分类索引/递归删除纯逻辑单测。
- 未完成：`Hotkey::label()` / `slint_key_to_vk()` / `ProcessInfo::label()` 迁移与全仓可见性收口。
- 未完成：PCM/WAV/时长/配置迁移测试和三个历史超限文件的拆分。

改动文件：
- `src/config/`、`src/hotkey.rs`、`src/audio/`、`src/engine/`、`src/cli/`、`src/ui_bridge/`。
- `ui/app-window.slint`、`ui/pages/settings.slint`。
- `scripts/check.cmd`、`scripts/check-lines.ps1`。
- `docs/step-39-refactor-s6.md`。

怎么跑：
- `cmd.exe /c "scripts\check.cmd"`
- `cmd.exe /c "scripts\build-dev.cmd test"`
- `cmd.exe /c "scripts\smoke.cmd"`

实测：
- 严格 Clippy `-D warnings` 通过。
- 构建通过。
- 领域单测通过：2 passed。
- 设备/进程 CLI 与两个 UI 快照冒烟通过。
- 行数门禁通过；3 个历史超限文件按基线未增长。

未覆盖：
- 真实声卡 route/capture 逐采样对比。
- Windows 桌面双击、拖动进度和右键交互手工验收。
- 资源基线 RAM/CPU/启动时间实测。

下一步：
- 完成余下的文件预算拆分、纯逻辑测试与基础设施文案映射迁移。
- 补测真实设备路径和资源基线后，才能将重构计划标记为完整验收。

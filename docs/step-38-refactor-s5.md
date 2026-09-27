# Step 38 — CLI 与平台基础设施拆分

范围：
- 做了：将 argv 派发、帮助、设备/进程/capture/route 命令和播放探针迁移到 `src/cli/mod.rs`。
- 做了：`main.rs` 收敛为 DPI、日志初始化和 CLI/UI 组合根。
- 做了：新增 `src/platform/com.rs`，把 COM apartment 初始化迁出音频原语；`audio::com` 仅作兼容转发，`process.rs` 改依赖 platform。
- 明确未做：config.rs 的 store/single-instance 拆分和 CLI 子文件进一步细分。

改动文件：
- `src/main.rs`、`src/cli/mod.rs`。
- `src/platform/mod.rs`、`src/platform/com.rs`、`src/audio/com.rs`、`src/process.rs`。
- `docs/step-38-refactor-s5.md`。

怎么跑：
- `cmd.exe /c "scripts\build-dev.cmd build"`
- `cmd.exe /c "scripts\smoke.cmd"`

实测：
- 构建通过。
- 设备/进程 CLI 输出和 UI 快照冒烟通过。
- 未执行真实 capture WAV 逐采样与 route 听感验收。

下一步：
- S6 收口 config store/single-instance、删除死代码、补齐纯逻辑单测并启用严格门禁。

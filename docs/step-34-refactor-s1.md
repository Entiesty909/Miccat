# Step 34 — UI 桥接层目录化

范围：
- 做了：把桥接单体移入 `src/ui_bridge/`，按共享状态、回调域、列表、持久化、
  状态轮询、窗口、文件对话框和播放辅助拆分。
- 做了：用 `AppState` 收敛回调入口；提取统一可见条目下标查询与 `stop_playback()`。
- 做了：回调配置保存改为 300 ms 单次防抖、FIFO 后台单写线程；退出时 flush 并 join。
- 明确未做：S2 引擎播放/试听门面；桥接仍直接调用解码、SFX、试听和设备策略。
- 明确未做：S0 资源基线测量、第三种（分类菜单）快照和 Windows 桌面音频全路径手工验收。

改动文件：
- `src/ui_bridge.rs` → `src/ui_bridge/mod.rs`。
- `src/ui_bridge/state.rs`、`cache.rs` 与 `callbacks/`（7 个回调域模块）。
- `src/ui_bridge/lists.rs`、`library.rs`、`format.rs`、`persist.rs`、`status.rs`、
  `input.rs`、`window.rs`、`dialogs.rs`、`playback.rs`、`dev/snapshot.rs`。
- `scripts/check.cmd`、`scripts/check-lines.ps1`、`scripts/smoke.cmd`、`.gitignore`。
- `FRONTEND.md`、`DEBUGGING.md`、`docs/step-02-rust-architecture.md`、`docs/refactor-plan.md`。

怎么跑：
- `cmd.exe /c "scripts\check.cmd"`
- `cmd.exe /c "scripts\smoke.cmd"`
- `cmd.exe /c "scripts\build-dev.cmd test"`

实测：
- 日期：2026-09-27；机器：Windows x64。
- `check.cmd` 通过；格式、Clippy（45 条告警，非阻断）、构建与行数门禁均执行完成。
- `cargo test` 的可见条目过滤/下标映射测试通过：1 passed。
- 隔离 `APPDATA` 的临时保存队列测试验证了 flush 写入最新配置快照；临时测试代码已移除。
- `smoke.cmd` 的设备/进程枚举通过；默认音频库与条目菜单快照均为 1080×560 RGBA。
- 未实测真实设备路由、双击播放、拖动进度与停止交互；快照不覆盖这些路径。

下一步：
- 完成 S0 尚缺的资源基线和快照状态记录。
- S2 将播放、试听与音频库领域操作收归安全引擎/领域 API。

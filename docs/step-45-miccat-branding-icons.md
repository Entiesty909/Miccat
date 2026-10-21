# Step 45 — Miccat 名称与源图标居中
范围：将项目标识统一迁移到 Miccat。功能验收：1) Cargo 包名与 Debug/Release exe、脚本命令一致；2) UI 标题、互斥体、配置和日志使用 Miccat 标识；3) 路由源图标按父卡片尺寸居中；4) 文档与冒烟命令同步新名称。不保留旧 `mintcat-proc-mic.exe` 产物路径。

改动文件：
- `Cargo.toml` / `Cargo.lock`：包名改为 `miccat`，debug 产物为 `miccat.exe`。
- `scripts/build-debug.cmd`、`scripts/build-release.cmd`、`scripts/smoke.cmd`、`scripts/dev.ps1`：切换到 `miccat.exe` 和 Miccat 临时快照命名。
- `src/main.rs`、`src/cli/mod.rs`、`src/hotkey.rs`、`src/config/single_instance.rs`、`src/config/store.rs`、`src/diagnostics/log.rs`、`src/ui_bridge/persist.rs`、分类拖拽标识：运行时名称、互斥体、配置/日志目录和进程类名改为 Miccat。
- `ui/app-window.slint`、`ui/pages/settings.slint`：窗口标题和设置页产品名改为 Miccat。
- `ui/components/source-row.slint`：图片坐标按父容器与自身尺寸计算，保持图标几何居中。
- `PROJECT.md`、`ARCHITECTURE.md`、`BACKEND.md`、`API.md`、`DEBUGGING.md`、`FRONTEND.md`、`README.md` 和本阶段文档：同步当前名称与命令。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\build-dev.cmd build
scripts\smoke.cmd
target\debug\miccat.exe --snapshot-ui --page route --out target\miccat-icon-check.raw
```

实测：2026-10-11，Windows x64。
- `fmt -- --check` 通过。
- `scripts\build-dev.cmd build` 通过，产物为 `target\debug\miccat.exe`。
- `scripts\smoke.cmd` 通过。
- `scripts\check.cmd` 通过；既有超限基线文件未增长。
- 路由软件渲染快照成功输出 1280x800。

下一步：若需要兼容旧版本配置，需另行设计 `%APPDATA%\ProcessMic` 到 `%APPDATA%\Miccat` 的一次性迁移；本轮按完整重命名直接使用 Miccat 路径。

# Step 79 — 紧凑路由卡片与 Windows 窗口图标
范围：路由卡片横向紧凑排列，列宽避免窄窗口溢出；图标和文字有明确间距；麦克风显示已选名称，不显示 dB；启用转发位于最右并垂直居中，操作按钮与启用区之间的竖分隔线也垂直居中。路由卡片与添加源列表保留实际进程 exe 图标；`resources/icon.ico` 只用于 Windows 标题栏左上角和 exe 资源。
改动文件：
- `ui/components/source-row.slint`：紧凑卡片布局；路由进程显示各进程真实 exe 图标，图文间距增加；操作/启用区分隔线及启用控件垂直居中。
- `ui/components/add-source-panel.slint`：进程选择列表恢复每项对应的实际 exe 图标。
- `ui/app-window.slint`：将 `resources/icon.ico` 绑定到 `MainWindow.icon`，显示在系统标题栏最左侧。
- `resources/icon.ico`、`resources/app.rc`、`build.rs`：同一 ICO 同时作为 Slint 窗口图标与 Windows exe 资源。
- `ui/components/app-header.slint`：不放置应用图标，页签恢复从左侧起始。
- `FRONTEND.md`：同步实际图标位置与用途。

怎么跑：
```bat
set CARGO_TARGET_DIR=target\verify-route-icons
scripts\build-dev.cmd fmt -- --check
scripts\build-dev.cmd build
scripts\build-dev.cmd test
scripts\check.cmd
target\verify-route-icons\debug\miccat.exe --snapshot-ui --page route --source-meter --out target\verify-route-icons\route-icons.raw
```

实测：2026-10-19，Windows x64。
- 独立目标目录构建成功；默认 `target\debug\miccat.exe` 被运行中的进程占用，未强制终止用户进程。
- `scripts\build-dev.cmd fmt -- --check` 成功。
- `scripts\build-dev.cmd test`：34 passed、0 failed。
- `scripts\check.cmd` 成功；仅有非阻断行数审查提示：`src/ui_bridge/callbacks/route/sources.rs` 650 行、`ui/app-window.slint` 472 行、`ui/components/add-source-panel.slint` 493 行。
- `resources/icon.ico` 已嵌入 exe 并绑定 `MainWindow.icon`；路由快照确认卡片显示实际进程图标，工具栏/页签栏不再放 app icon。根目录原始附件因 Windows `WinError 32` 文件锁仍无法删除。

下一步：真实桌面确认长设备名、多个卡片同时显示和启用开关点击区域；解除根目录附件锁后删除重复的根目录 `icon.ico`。

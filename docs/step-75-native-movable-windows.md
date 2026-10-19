# Step 75 — 独立可移动窗口与音频库交互修复

范围：主窗口首次显示居中、关闭后恢复上次位置；添加源和编辑窗口首次显示居中、关闭/隐藏后恢复各自位置。修复独立窗口第二次显示白屏，内容区使用原生系统标题栏且非模态、可拖动。添加音频文件到路由后关闭添加源窗；添加进程不启动全局转发。保留分隔条、图标映射、状态栏Popover锚点、dB显示和真实数据同步。

改动文件：`src/config/mod.rs`、`src/config/store.rs`、`src/config/single_instance.rs`、`src/platform/mod.rs`、`src/platform/window.rs`、删除 `src/platform/dialog_window.rs`、`src/ui_bridge/window.rs`、`src/ui_bridge/state.rs`、`src/ui_bridge/mod.rs`、`src/ui_bridge/status.rs`、`src/ui_bridge/dialogs.rs`、`src/ui_bridge/callbacks/library.rs`、`src/ui_bridge/callbacks/library_entries.rs`、`src/ui_bridge/dev/snapshot.rs`、`src/cli/mod.rs`、`scripts/smoke.cmd`、`ui/windows.slint`、`ui/components/add-source-panel.slint`、`ui/components/audio-entry-editor.slint`、`ui/components/library-splitter.slint`、`ui/components/status-bar.slint`、`ui/pages/route.slint`、`ui/pages/player.slint`、`ui/app-window.slint`、相关契约文档。

关键行为：独立Window保留Windows系统标题栏与可拖动行为，不设置owner、不禁用主窗；`AppConfig.windows` 持久化三个窗口关闭时的物理屏幕坐标。首次主窗待尺寸稳定后以Win32外框精确居中；新建子窗按外框居中，后续恢复保存位置。窗口hide/show切换全窗刷新面并请求重绘，避免软件渲染器只重绘鼠标悬停控件。单实例互斥体创建前清零LastError，避免旧错误码误判已有实例。

怎么跑：
- `scripts\build-dev.cmd fmt`
- `scripts\build-dev.cmd test`
- `scripts\check.cmd`
- `scripts\smoke.cmd`
- `target\debug\miccat.exe --snapshot-ui --page route --out <temp>\route.raw`
- `target\debug\miccat.exe --snapshot-ui --page route --source-meter --out <temp>\meter.raw`
- `target\debug\miccat.exe --snapshot-ui --page route --source-dialog-tab process --source-dialog-reopen --out <temp>\reopen.raw`
- `target\debug\miccat.exe --snapshot-ui --page player --out <temp>\player.raw`

实测：32 项单测、`scripts\check.cmd`、`scripts\smoke.cmd` 通过；烟测覆盖窗口hide/show像素一致性、三类添加源Tab、音源菜单和声纹Popover。Windows实际测试确认主窗首次居中误差 `(0, 0)`，设置关闭位置后下一次启动还原；添加源窗首次居中误差 `(0, 0)`，标题栏拖动后还原所存位置。AddSourceWindow关闭再打开后的实际截图像素差异均值小于 `0.000001`；AudioEditWindow实际显示后可拖动、native close后主窗仍可用。音频条目添加到路由后子窗隐藏。进程添加测试中全局状态仍为“未运行”，但新增进程源卡开关为启用。自定义图标文件选择/保存和真实设备出声未在本轮完成验证。非阻断行数审查警告：`ui/app-window.slint` 471 行、`ui/components/add-source-panel.slint` 493 行。

下一步：如果“添加进程后默认开启”指新增源卡片的启用开关（当前实测为开启），需确认是否改为默认关闭；全局转发在测试中保持停止。真实设备出声与自定义图标保存仍需验收。
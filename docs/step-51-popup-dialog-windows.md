# Step 51 — 独立窗口与重复显示生命周期收口

范围：同一批收口添加源窗口、音频库编辑窗口的呈现、拖动、位置保存、重复显示重绘和提交后隐藏。实现过程先将主窗内嵌层改为 PopupWindow，随后根据原生标题栏/可拖动需求切换为独立 Windows `Window`；最终以系统标题栏、非模态、可拖动的原生窗口为准。

改动文件：
- `ui/windows.slint`、`ui/components/{add-source-panel,audio-entry-editor,library-splitter,status-bar}.slint`、`ui/pages/{route,player}.slint`、`ui/app-window.slint`：独立窗口内容、拖动、路由源弹层与页面行为。
- `src/config/{mod.rs,store.rs,single_instance.rs}`、`src/platform/{mod.rs,window.rs}`、`src/ui_bridge/{window.rs,window_lifecycle.rs,state.rs,mod.rs,status.rs,dialogs.rs}`、相关 callbacks 和 snapshots：窗口位置、生命周期、显示前定位及重绘。
- `scripts/smoke.cmd`、`PROJECT.md`、`FRONTEND.md`、`ARCHITECTURE.md`、`DEBUGGING.md`：同步窗口契约和烟测。

最终行为：
- 主窗口、添加源窗、编辑窗分别保存/恢复物理像素坐标；首次显示按窗口外框居中，后续启动恢复位置。
- 子窗无 owner、不禁用主窗；关闭按钮隐藏窗口，原生标题栏可拖动。隐藏后再显示强制请求整窗重绘，避免软件渲染只刷新悬停控件。
- 添加进程/音频/麦克风的提交只更新对应源并隐藏添加窗，不自动启动全局路由；窗口再次打开沿用保存位置。
- 分类/音频编辑继续复用原有 callback 和配置模型；PopupWindow 源菜单与可移动顶层窗口职责分开。

怎么跑：
- `scripts\build-dev.cmd fmt`
- `scripts\build-dev.cmd test`
- `scripts\check.cmd`
- `scripts\smoke.cmd`
- `target\debug\miccat.exe --snapshot-ui --page route --source-dialog-tab process --source-dialog-reopen --out "%TEMP%\miccat-reopen.raw"`
- `target\debug\miccat.exe --snapshot-ui --page player --out "%TEMP%\miccat-player.raw"`

实测：2026-10-19，Windows x64。
- 最终构建、`scripts\check.cmd`、`scripts\smoke.cmd` 和 32 项单元测试通过；reopen 软件快照隐藏/重开像素一致性通过。
- Windows 桌面实测主窗和添加源窗首次定位误差均为 `(0, 0)`，窗口拖动后位置可恢复；编辑窗可拖动、关闭后主窗仍可用。
- 添加音频后子窗隐藏；进程添加未启动全局转发。真实设备出声和自定义图标文件选择/保存未在该批实测。
- 行数门禁仅报告既有审查阈值提示，硬上限通过。

下一步：真实设备声音和自定义图标文件操作属于独立验收目标；同批窗口显示/隐藏修复应补入本记录，不另建 Step。

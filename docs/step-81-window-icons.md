# Step 81 — 窗口应用图标统一

范围：为添加音频源窗口、音频库条目/分类编辑窗口设置 `resources/icon.ico`；将同一 ICO 嵌入 Windows exe 的数字资源 ID 1，以供 Windows 任务栏使用。未改变窗口行为和内容。

改动文件：
- `ui/windows.slint`：`AddSourceWindow` 与 `AudioEditWindow` 设置窗口图标；分类与条目编辑共用该窗口。
- `resources/app.rc`：资源 ID 明确指定为数字 ID 1。
- `FRONTEND.md`、`PROJECT.md`：同步图标行为与本阶段索引。

怎么跑：
- `cmd.exe /c scripts\\build-dev.cmd build`
- `cmd.exe /c scripts\\smoke.cmd`

实测：2026-10-19，Windows x64。本地 Debug 构建成功；冒烟脚本设备/进程枚举和全套 UI 快照成功，其中包括添加源窗口三种类型及重新显示快照。该快照为软件渲染客户区，不包含 Windows 原生标题栏/任务栏；标题栏和任务栏图标未通过人工桌面截图单独核验。

下一步：无需代码跟进；若需要像素级确认 Windows 壳层图标，需在真实桌面窗口/任务栏上检查。

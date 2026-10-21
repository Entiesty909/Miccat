# Step 54 — 主窗口三栏与状态栏交互收口

范围：同一批完成全宽顶栏/内容/底栏三行布局、侧栏页面切换、状态栏控件顺序和尺寸、总音量纯文字提示及刷新后失效源清理。页面入口移至 45px 图标侧栏；状态栏最终 48px。品牌/按钮垂直居中；总音量提示在主布局外跟随全局鼠标坐标，可越出窗口。不保留中途试验的 30px 侧栏/32px 底栏规格。

改动文件：
- `ui/app-window.slint`、`ui/components/{app-header,sidebar-nav,status-bar,button}.slint`、`ui/pages/{route,player,settings}.slint`：三栏结构、控件排列、固定滑块父组和 PopupWindow 提示。
- `src/platform/window.rs`、`src/ui_bridge/{mod.rs,state.rs,status.rs,dev/snapshot.rs}`：窗口坐标、版本文本、50ms 拖动坐标轮询及状态刷新。
- `src/ui_bridge/callbacks/route/refresh.rs`、相关 route callbacks：后台刷新后清除已失效文件源、关联槽位和配置。
- `Cargo.toml`：使用既有 `windows` crate 的 GDI 坐标 API；没有新增依赖。
- `FRONTEND.md`、`ARCHITECTURE.md`、`DEBUGGING.md`、`PROJECT.md`：同步窗口和状态栏契约。

最终行为：
- 顶栏和底栏横跨窗口全宽；侧栏只占中间内容行，点击切换路由/音频库/设置，不显示 Tooltip。
- 状态栏音源入口在总音量滑块之前；滑块保持固定父组宽度，拖动时提示不参与布局。
- 提示由 Win32 `GetCursorPos`/`ScreenToClient` 定位，离开主窗口仍跟随鼠标；滑块释放后关闭。
- 刷新共用后台进程/设备枚举；文件丢失、PID 或选中麦克风失效时清理对应路由状态，不在 UI 线程同步枚举。

怎么跑：
- `scripts\build-dev.cmd fmt -- --check`
- `scripts\build-dev.cmd test`
- `scripts\check.cmd`
- `scripts\smoke.cmd`
- `target\debug\miccat.exe --snapshot-ui --page route --out "%TEMP%\miccat-route-layout.raw"`
- `target\debug\miccat.exe --snapshot-ui --page settings --out "%TEMP%\miccat-settings-layout.raw"`

实测：2026-10-21，Windows x64。
- Debug 构建、完整门禁和 38 项单元测试通过；路由/设置页 1280×800 软件渲染快照确认三栏布局，侧栏只覆盖中间行。
- 快照确认顶栏/底栏控件对齐，错误详情在左、音量与音源在右；刷新源清理路径由后台枚举与 UI 应用结果分开。
- 未进行真实鼠标拖动/拖出窗口跟随、逐项刷新点击或人为移除活动文件的桌面验收。

下一步：总音量跨窗口拖动与文件/设备失效桌面验收；这批布局/状态栏回归继续更新本记录。

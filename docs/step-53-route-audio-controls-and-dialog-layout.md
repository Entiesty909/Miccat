# Step 53 — 路由音频控制与添加源窗口收口

范围：统一路由文件卡片的播放/暂停/停止、音频/麦克风添加源布局和麦克风提交行为。文件槽位互相独立；关闭文件转发不误启全局路由。添加物理麦克风成功后隐藏窗口，路由卡片显示固定“麦克风”标题和所选设备友好名称；普通入口每次从“进程”开始，替换入口保留目标源类型。未改变麦克风捕获和默认设备策略。

改动文件：
- `src/audio/sfx.rs`、`src/engine.rs`、`src/engine/{output,playback}.rs`：追踪 SFX 槽位状态，支持暂停/恢复和停止后从头播放。
- `src/ui_bridge/sources.rs`、`src/ui_bridge/callbacks/route.rs`、`src/ui_bridge/callbacks/route/sources.rs`、`src/ui_bridge/status.rs`、`src/ui_bridge/mod.rs`、`src/ui_bridge/persist.rs`、`src/ui_bridge/dev/snapshot.rs`：每槽状态投影、麦克风提交/替换和配置保存。
- `ui/components/{source-row,add-source-panel,audio-source-list,audio-source-details,source-actions-menu}.slint`、`ui/windows.slint`、`ui/pages/route.slint`、`ui/app-window.slint`：槽位控制、音频/麦克风双栏、列表排列和窗口关闭/页签重置。
- `ARCHITECTURE.md`、`BACKEND.md`、`API.md`、`FRONTEND.md`、`DEBUGGING.md`、`PROJECT.md`：同步槽位和窗口行为契约。

怎么跑：
- `scripts\build-dev.cmd fmt -- --check`
- `scripts\build-dev.cmd test`
- `scripts\check.cmd`
- `scripts\smoke.cmd`
- `target\debug\miccat.exe --snapshot-ui --page route --out "%TEMP%\miccat-route.raw"`
- `target\debug\miccat.exe --snapshot-ui --page route --source-dialog-tab microphone --out "%TEMP%\miccat-microphone.raw"`

实测：2026-10-20，Windows x64。
- 格式检查、完整门禁、Smoke 和 35 项单元测试通过；软件快照覆盖 1280×800 路由页与 698×620 麦克风添加源窗口。
- 快照核对麦克风设备列表靠顶、输出设置可见、路由卡片显示“麦克风”及实际设备友好名称；单元测试验证文件暂停保持位置、恢复继续输出缓冲样本。
- 未声称完成真实 Render 设备听感、物理鼠标提交/替换或关闭软件重启后的源卡状态验收。

下一步：真实设备验证文件槽位控制和麦克风混入/停止；同批添加源布局与提交修复更新本记录，不另建阶段文档。

# Step 53 — 路由音频控制与添加源窗口收口

范围：同批收口路由页三类卡片与文件槽控制。进程、音频、麦克风卡片改为参考图的左侧源信息、右侧输出/转发音量、最右启用开关；进程和音频显示禁用的预览音量占位，不实现预览后端。原 `...` 改为右上角上三角。音频卡片显示精确音频库分类，提供上一首、播放/暂停、下一首，以及同一按钮循环切换顺序播放、列表循环、单曲循环、随机播放。文件槽位互相独立；启用转发不自动播放。添加音频源入口及原打开窗口逻辑移到顶栏，路由页删除“刷新进程”按钮。未改变麦克风捕获、默认设备和全局刷新策略。

改动文件：
- `src/audio/sfx.rs`、`src/engine.rs`、`src/engine/{output,playback}.rs`、`src/cli/mod.rs`：为每个 SFX 槽发布播放状态和 100 ns 位置，保留独立暂停/恢复/停止。
- `src/config/mod.rs`、`src/ui_bridge/cache.rs`、`src/ui_bridge/persist.rs`：按文件槽持久化四种播放模式，并维持条目、模式、完成态数组对齐。
- `src/ui_bridge/callbacks/route/audio_queue.rs`：按当前条目的精确分类实现前后切歌和自然结束续播；跳过其它卡片已占用条目。
- `src/ui_bridge/{sources,status}.rs`、`src/ui_bridge/callbacks/{mod,route}.rs`、`src/ui_bridge/callbacks/route/sources.rs`、`src/ui_bridge/dev/snapshot.rs`：投影分类、模式、进度、时长，注册事件并为快照补齐三类源。
- `ui/components/{source-row,slider-row,source-actions-menu,app-header,page-host}.slint`、`ui/pages/route.slint`、`ui/app-window.slint`：三类卡片布局、禁用预览占位、右上角菜单、文件控制和顶栏添加源入口。
- `ARCHITECTURE.md`、`BACKEND.md`、`API.md`、`FRONTEND.md`、`DEBUGGING.md`、`PROJECT.md`：同步文件槽队列、卡片和入口契约。

怎么跑：
- `scripts\build-dev.cmd fmt -- --check`
- `scripts\build-dev.cmd test`
- `scripts\build-dev.cmd build`
- `scripts\check.cmd`
- `scripts\smoke.cmd`
- `target\debug\miccat.exe --snapshot-ui --page route --out "%TEMP%\miccat-route.raw"`
- `target\debug\miccat.exe --snapshot-ui --page route --source-menu-click --out "%TEMP%\miccat-route-menu.raw"`

实测：2026-10-22，Windows x64。
- 44 项单元测试通过，包含模式切换顺序、首尾环绕、随机避开当前条目和 SFX 暂停位置。
- 默认目标目录构建因正在运行的 `target\debug\miccat.exe` 被 Windows 锁定而无法覆盖；使用隔离目标目录继续验证，不将此项记为代码失败。
- 路由卡片快照、菜单命中、完整门禁和 Smoke 结果见本阶段最终验收记录。
- `source-row.slint` 为三类卡片共享组件，440 行，低于 450 行审查阈值；`sources.rs` 774 行，仍低于 800 行硬上限。桥接并行模型集中维护以保留下标不变量，本轮未再拆第二套构造器。
- 未声称完成真实 Render 设备听感或物理鼠标操作验收。

下一步：真实设备验证分类续播、文件槽控制和麦克风混入/停止；同批卡片调整继续更新本记录，不另建阶段文档。

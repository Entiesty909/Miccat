# Step 50 — 多源路由卡片与麦克风关停收口

范围：同一批完成多进程/多音频添加替换、独立卡片状态、重复源拒绝、文件手动启停、空闲声纹性能和麦克风捕获安全关停。保留全局总音量和路由入口语义，不改变真实麦克风的捕获角色。

改动文件：
- `ui/components/{source-actions-menu,source-row,add-source-dialog}.slint`、`ui/pages/route.slint`：添加、替换、删除和单卡控制。
- `src/config/mod.rs`、`src/engine.rs`、`src/engine/{output,fanout,volume,playback}.rs`、`src/audio/{render,sfx,preview}.rs`：槽位增益/输出路由、并行 SFX 流和捕获线程停止标志。
- `src/ui_bridge/{cache,mod,persist,sources,status,state}.rs`、`src/ui_bridge/callbacks/{route,route/sources,route_start,route_start/source_only,library_entries}.rs`、`src/ui_bridge/dev/snapshot.rs`、`src/cli/mod.rs`：槽位状态、重复校验、单独转发、刷新和烟测。
- `scripts/smoke.cmd`、`ARCHITECTURE.md`、`BACKEND.md`、`FRONTEND.md`、`DEBUGGING.md`、`API.md`、`PROJECT.md`。

功能要求与最终语义：
1. 多进程/多音频可同时加入、单卡替换和删除；文件启用不自动播放，点击卡片才开始对应流，单独转发只操作目标槽。
2. 每张进程/文件卡独立保存增益、静音恢复、输出 ID 和 SFX 状态；添加/替换前拒绝重复 PID/音频条目。
3. 路由菜单删除后立即刷新并保留其它行，更多按钮不被滚动区裁剪；麦克风下拉固定宽并与名称列对齐。
4. 空闲声纹轮询只在采样/历史变化时更新模型；启用麦克风后停止路由/运行时切换必须先设置保存的 stop 原子，再 join，最多等待 80ms 事件超时。

根因记录：`AudioEngine::set_mic_source()` 曾只把 stop 标志 clone 给捕获线程，却没保存原子到 `SourceThreads.mic_stop`；因此 `stop_mic_source()` 不能通知捕获循环退出而永久 join。现在设置和停止共享同一 stop 标志。

怎么跑：
- `scripts\build-dev.cmd fmt`
- `scripts\build-dev.cmd test`
- `scripts\check.cmd`
- `scripts\smoke.cmd`
- `target\debug\miccat.exe --snapshot-ui --page route --source-menu-click --out "%TEMP%\miccat-source-menu.raw"`
- `target\debug\miccat.exe --snapshot-ui --page route --source-dialog-click-tab microphone --out "%TEMP%\miccat-source-tabs.raw"`

实测：2026-10-19，Windows x64。
- 多源功能门禁通过：26 项单元测试、`scripts\check.cmd`、`scripts\smoke.cmd`；空闲独立测量后 2 秒 CPU 增量约 0.015625 秒（约 0.78% 单核），仅代表本机一次无音频快照。
- 麦克风停止修复构建/门禁/26 项单测通过，等待链核对为 stop 原子先置位、最多 80ms 后 join；未声称完成真实物理麦克风听感演练。
- 卡片隔离最终构建、check、smoke 和 34 项单元测试通过，覆盖重复进程/音频添加与替换边界；真实多源听感、拔设备及桌面点击仍需实机验收。

下一步：在真实设备验证多卡音量/静音/输出互不干扰、物理麦克风停止与热切换；同批路由源状态补丁更新本记录，不另建 Step。

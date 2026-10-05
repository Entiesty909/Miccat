# Step 45 — 参考图驱动的前端和音源逻辑
本阶段后续修正：对照用户提供的目标截图，移除旧路由页卡片/页脚遗留结构，改为对齐的进程、文件、麦克风源行；独立输出设备选择接入配置与引擎 fanout，文件行补暂停/停止，底部状态栏改为运行状态、音源数量、总音量和启停。
独立输出的每行设备选择现在保存到 `OutputConfig`，路由启动时映射为 `SourceOutputRoutes`，fanout 仅向对应 render 目标投递该来源。
本次修正：音频库改为参考图的三栏结构（分类/音源列表/当前条目详情）；详情展示本机条目元数据，桌面空闲时隐藏播放控制条，播放/暂停时再显示。
详情“本机试听”已接到 `PlaybackTarget::Monitor`；主音量滑块只持久化 `preview_volume` 并调整本机 monitor，不改路由 SFX 增益。
软件快照改为枚举真实 render 设备，并按配置 ID 恢复文件输出项和本机监听音量；不再用“未选择输出设备”占位。

第二页最终按目标图拆为分类计数/快捷键侧栏、搜索与名称排序/分页文件表、文件详情和底部播放器。文件大小、采样率、声道、估算码率和路径使用本地导入元数据；音频快捷键持久化且只在程序获焦时触发。

改动文件：
- `ui/pages/route.slint`、`ui/components/source-row.slint`：进程、文件、麦克风独立音源行；每行输出选择、增益、静音及文件暂停/停止接入现有桥接控制。
- `ui/pages/player.slint`、`ui/components/page-host.slint`、`audio-list.slint`、`audio-detail.slint`、`audio-transport.slint`、`category-sidebar.slint`：参考图三栏布局、真实元数据和播放状态控件。
- `ui/pages/settings.slint`：可切换常规/音频设备/快捷键/VB-CABLE/关于分组。
- `ui/components/app-focus-scope.slint`、`src/ui_bridge/callbacks/library_view.rs`：捕获/触发持久化音频快捷键；在应用获焦时播放，重复和路由按键冲突拒绝。
- `src/ui_bridge/library.rs`、`src/config/mod.rs`、`persist.rs`、`callbacks/library_entries.rs`：分页过滤映射；文件大小、采样率、声道和时长在导入/迁移时持久化。
- `src/engine/output.rs`、`src/engine/fanout.rs`、`callbacks/route_start.rs`：按进程/文件/麦克风独立 render 目标分流。
- `src/ui_bridge/status.rs`、`sources.rs`、`lists.rs`、`callbacks/route.rs`：真实引擎状态、设备/进程枚举和音源行操作。
- `src/cli/mod.rs`：播放探针忽略生产端初始静音并扩大 seek 附近对齐窗口，避免把线程启动延迟报告成采样错位。
- `src/ui_bridge/dev/snapshot.rs`：快照枚举实际输出设备并恢复配置中的输出 ID/监听增益。
- `FRONTEND.md`、`BACKEND.md`、`DEBUGGING.md`、`ARCHITECTURE.md`、`FILE-CONVENTIONS.md`、`PROJECT.md`。

怎么跑：
- `cmd.exe /c "scripts\build-dev.cmd fmt -- --check"`
- `cmd.exe /c "scripts\build-dev.cmd clippy --target-dir target-ui -- -D warnings"`
- `cmd.exe /c "scripts\build-dev.cmd test --target-dir target-ui"`
- `cmd.exe /c "scripts\build-dev.cmd build --target-dir target-ui"`
- `target-ui\debug\mintcat-proc-mic.exe --snapshot-ui --page route|player|settings --out <file>`
- 对最终二进制运行 `--snapshot-ui --menu`、`--category-menu`、`--category-row-menu`、`--list-devices`、`--list-processes`。

实测：
- 2026-09-29，Windows x64：格式检查、严格 Clippy、target-ui 构建和 18 项测试通过；`smoke.cmd` 完成设备/进程枚举及四类 UI 快照。
- route/player/settings 1280×800 快照生成成功；player 详情显示本地音频名、元数据、路径、分类和配置中的输出设备。
- 合成 6 秒 WAV 经 SFX 探针：启动静音剔除后比对 515086 个样本，0 不匹配；2 秒 seek 样本 250486 个，0 不匹配；本机试听 523906 个样本，0 不匹配。
- 播放条主音量已改为只作用于本机 monitor；本机试听按钮绑定 `PlaybackTarget::Monitor`。真实 VB-CABLE 听感和桌面点击未验收。
- `check-lines.ps1` 仍因 `src/library.rs` 525/400 失败；`src/audio/capture.rs` 449/400、`ui/app-window.slint` 383/300 为不增长基线。

下一步：
- 在真实桌面回归进程加入/移除、输出 ID 恢复、多输出路由、文件循环/seek/停止、本机监听、麦克风混入和各源静音恢复。

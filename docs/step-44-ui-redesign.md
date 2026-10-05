# Step 44 — 三页界面重排
范围：按效果图重排路由、音频库和设置页；顶部状态、进程/设备名、音频条目、时长与播放状态继续来自现有配置、枚举和 AudioEngine。既有操作接回原回调，不改变音频引擎的混音/路由语义，不显示无实现的启动、托盘或主题选项。

改动文件：
- `ui/theme.slint`：统一浅色背景、圆角、边框、强调色和紧凑尺寸。
- `ui/app-window.slint`、`ui/components/app-header.slint`：顶栏显示实时引擎状态，窗口默认尺寸调整为 1280×800，最小尺寸保持 900×600。
- `ui/pages/route.slint`、`ui/components/status-bar.slint`：分区展示实际进程、当前输出和物理麦信息；混麦使用连接到既有回调的开关。
- `ui/pages/player.slint`、`ui/components/audio-list.slint`、`ui/components/category-sidebar.slint`：三栏音频库、真实选中项信息、底部播放控制与本机监听音量；空分类显示真实提示。
- `ui/pages/settings.slint`、`ui/components/button.slint`、`ui/components/card.slint`：可切换设置分组与扁平导航样式；既有设备、默认设备、热键和 VB-CABLE 回调保持接线。
- `src/ui_bridge/dev/snapshot.rs`：快照使用 1280×800，不再注入示例歌曲名/播放位置；分类行右键模拟坐标适配新页面。
- `FRONTEND.md`、`docs/step-02-rust-architecture.md`：同步当前页面结构与功能边界。

怎么跑：
- `cmd.exe /c "scripts\build-dev.cmd fmt -- --check"`
- `cmd.exe /c "scripts\check.cmd"`
- `cmd.exe /c "scripts\smoke.cmd"`
单页快照：`target-ui\debug\mintcat-proc-mic.exe --snapshot-ui --page route|player|settings --out <file>`

实测：
- 2026-09-28，Windows x64：Rust 格式检查、Clippy `-D warnings` 和标准构建通过；`scripts\check.cmd` 最终因既有行数门禁失败：`src/library.rs` 521/400 行，`src/engine.rs` 432/400 与 `src/audio/capture.rs` 451/400 为已登记的不增长基线。
- 最后一次标准 target 构建被仍在运行的 `mintcat-proc-mic.exe` 锁定；改用 `target-ui` 隔离目录构建通过，路由/音频库/设置三页最终版本均成功渲染 1280×800 快照。
- `scripts\smoke.cmd` 通过：设备/进程枚举成功；默认页、条目菜单、空分类菜单、分类行菜单四张快照均成功，尺寸 1280×800。
- 快照使用本机真实进程/设备和当前本地音频配置，不注入演示歌曲名或播放位置。
- 未验证真实路由听感、物理鼠标操作与各播放设备的本机监听；软件快照不替代这些桌面实测。

下一步：
- 在真实窗口操作设置分组、设备选择、混麦开关、音频播放/暂停/停止/进度与右键菜单；按 `goal.md` 继续实机验收音频链路。

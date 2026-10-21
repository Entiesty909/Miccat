# Step 41 — 三页 UI 重设计与音频交互接线收口

范围：同一批完成路由、音频库、设置页的参考图重排及真实操作接线；后续播放器控制、工具栏、音源输出与路由交互修正合并到本记录。Slint 仅展示状态并复用既有 engine/config 行为；不引入样例进程、设备或音频条目。

功能要求：
1. 路由页显示真实进程/文件/麦克风源、输出选择、增益、静音和刷新；启停与麦克风混入接既有回调，输出目标按源行配置分发。
2. 音频库使用分类/列表/详情视图，展示本地文件元数据；播放器支持路由、本机监听、暂停/恢复、上一首/下一首、循环与进度，不在 UI 编造播放状态。
3. 设置页按设备、默认设备、麦克风、虚拟线缆、热键/预览与关于分组；撤除未接线的假控件，保留真实 callback。
4. 路由源卡操作/菜单按槽位索引接线；刷新在后台枚举后应用结果；多进程、文件菜单、麦克风选择和混音状态不串槽。
5. 音频播放恢复保证 monitor 就绪；对当前播放条目再次双击使用暂停/恢复，不将暂停伪装成停止/重新解码。

同批迭代修复：
- 三页按 Windows 视觉重排，窗口默认 1280×800，最小 900×600；音频库控制栏使用系统图标并恢复添加文件入口。
- 路由源卡片与添加源面板使用滚动/对话框；输出设备固定列宽，长名称在视口内滚动，顶部提供添加入口。
- 播放目标明确为两路、本机监听或活动路由；恢复监听前同步确保 monitor 已就绪，投递失败显示可见错误。
- 搜索/工具栏与真实条目列表在后续布局迭代中有增删调整；最终行为以 `FRONTEND.md` 和当前实现为准。
- 路由刷新修复进程行→PID 映射、文件菜单→当前可见条目映射及麦克风设备 ID 持久化；后台刷新不在 Slint 回调枚举。

改动文件：
- `ui/theme.slint`、`ui/app-window.slint`、`ui/pages/{route,player,settings}.slint`、`ui/components/{button,card,tab-bar,source-row,audio-list,audio-detail,audio-transport,category-sidebar,status-bar,app-header,app-focus-scope}.slint`、相关菜单/列表控件。
- `src/engine.rs`、`src/engine/{monitor,playback,fanout,output}.rs`、`src/ui_bridge/{cache,library,sources,status,lists,persist,input}.rs`、`src/ui_bridge/callbacks/`、`src/library.rs`、`src/hotkey.rs`、`src/cli/mod.rs`、`src/ui_bridge/dev/snapshot.rs`。
- `FRONTEND.md`、`BACKEND.md`、`ARCHITECTURE.md`、`API.md`、`DEBUGGING.md`、`FILE-CONVENTIONS.md`、`PROJECT.md`。

怎么跑：
- `scripts\build-dev.cmd fmt -- --check`
- `scripts\build-dev.cmd clippy --target-dir target-ui -- -D warnings`
- `scripts\build-dev.cmd test --target-dir target-ui`
- `scripts\build-dev.cmd build --target-dir target-ui`
- `scripts\smoke.cmd`
- `target-ui\debug\miccat.exe --snapshot-ui --page route|player|settings --out <file>`
- `target-ui\debug\miccat.exe --snapshot-ui --menu --category-menu --category-row-menu --out <file>`

实测：2026-09-29，Windows x64；同批迭代跨数次实现/回归。
- 最终综合阶段严格格式、Clippy、构建和 18 项单元测试通过；smoke 设备/进程枚举及四类 UI 快照通过。
- Route/Player/Settings 快照均为 1280×800；`--probe-playback` 对 515086 个播放样本、250486 个 seek 样本和 523906 个 monitor 样本逐采样比较均 0 不匹配。
- 后续路由接线审查补充的 21 项测试、严格 Clippy 和 route 快照通过；索引测试覆盖多进程槽、文件当前页映射和无效进程过滤。
- 真实虚拟线缆听感、物理鼠标交互、多输出和设备热拔插未由这些软件快照/离线探针完全代替，仍需桌面设备验收。

下一步：以当前功能契约在真实设备验证路由声音、监听、拖动进度和菜单交互；同批 UI/回调补丁更新本记录，不另建 Step。

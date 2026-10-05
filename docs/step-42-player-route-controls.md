# Step 42 — 路由与播放器交互调整
范围：按最新交互要求移除路由进程搜索和设置页混麦/本机试听入口；混麦设备选择放在状态栏，本机试听与音量放在音频库播放器控制条。新增播放器目的地选择、暂停恢复和上一首/下一首。音频解码格式、WASAPI 捕获/渲染和配置格式未更改。

改动文件：
- `ui/pages/route.slint`：删除搜索控件与重复进程下拉；进程来源/输出设备两栏固定均分，保留刷新、加入、清空和音量。
- `ui/components/status-bar.slint`、`ui/app-window.slint`：混麦勾选后显示麦克风选择下拉；状态栏仍保留启停和总音量。
- `ui/pages/settings.slint`：删除混麦麦克风选择和本机试听设置组。
- `ui/pages/player.slint`、`ui/components/audio-list.slint`：顶部播放控制、进度、循环、本机试听开关与音量；音频列表固定填充滚动视口，选中淡蓝优先于播放淡灰。
- `ui/components/app-focus-scope.slint`：从主窗口抽出热键捕获与 Esc 菜单关闭逻辑。
- `src/engine.rs`、`src/engine/playback.rs`：新增 `PlaybackTarget::{Both, Monitor, Route}`，允许路由输出、本机监听或同时播放。
- `src/ui_bridge/callbacks/player.rs`、`cache.rs`：连接目的地按钮、暂停位置恢复、前后曲切换和播放中的条目索引。
- `src/ui_bridge/lists.rs`、`callbacks/route.rs`、`callbacks/mod.rs`：移除进程筛选及不可达元数据编辑回调；进程下拉列出完整枚举。
- `src/ui_bridge/dev/snapshot.rs`、`src/cli/mod.rs`：支持 `--snapshot-ui --page route|player|settings`。
- `FRONTEND.md`、`BACKEND.md`、`ARCHITECTURE.md`、`API.md`、`DEBUGGING.md`、`FILE-CONVENTIONS.md`：同步页面与播放目标契约。

怎么跑：
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\build-dev.cmd fmt -- --check"`
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\build-dev.cmd clippy --target-dir target-ui -- -D warnings"`
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\build-dev.cmd build --target-dir target-ui"`
- `target-ui\debug\mintcat-proc-mic.exe --snapshot-ui --page route --out %TEMP%\processmic-route.raw`
- 同命令分别以 `--page settings`、默认 player、`--menu`、`--category-menu`、`--category-row-menu` 生成快照。

实测：
- Slint/Rust 构建通过；格式和严格 Clippy 检查通过。
- route、settings、player 与三类播放器/分类菜单快照均渲染为 1080×600；路由双栏等宽；音频列表第三行及之后位于滚动视口内。
- 列表颜色逻辑是选中淡蓝优先、播放条目未选中时淡灰。
- 播放按钮目标映射为：0 双路，1 本机监听，2 活动路由 SFX；暂停保存引擎当前位置，恢复时从该位置重启解码流。
- 未验证真实虚拟线缆/扬声器听感、热插拔与物理鼠标交互；需要 Windows 音频设备桌面验收。
- 完整 `scripts/check.cmd` 的行数门禁仍会因历史 `src/library.rs` 超过 400 行报错；本轮未重构该无关模块。

下一步：
- 桌面验证混麦勾选后的设备切换，以及播放目标三种模式、暂停/恢复、停止和试听音量。
- 独立处理既有 `src/library.rs` 行数超限，避免与本轮 UI 改动混在同一切片。

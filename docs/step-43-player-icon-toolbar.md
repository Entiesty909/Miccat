# Step 43 — 音频库图标工具栏与移除搜索
范围：播放器控制改用 Windows 原生图标字体；将现有“添加音频文件”操作移到播放器控制栏；移除音频名称搜索 UI、桥接状态与分类列表搜索逻辑。没有新增文件夹导入功能：当前源码原有按钮只选择单个音频文件。

改动文件：
- `ui/components/button.slint`：支持图标字体按钮。
- `ui/pages/player.slint`：播放、扬声器、麦克风、暂停、停止、上一首、下一首及添加文件按钮使用 Segoe MDL2 Assets 图标；删除搜索框。
- `ui/app-window.slint`、`src/ui_bridge/callbacks/library_entries.rs`、`src/ui_bridge/cache.rs`：移除音频搜索属性、回调和缓存。
- `src/ui_bridge/library.rs`、`src/library.rs`：列表按所选分类及后代展示，不再按名称过滤；更新可见索引单测。
- `src/ui_bridge/dev/snapshot.rs`：更新分类行右键快照坐标以适配删除搜索栏后的布局。
- `FRONTEND.md`、`API.md`、`DEBUGGING.md`、`docs/step-02-rust-architecture.md`、`docs/ui-redesign-proposal.md`：同步当前 UI 契约。

图标选择：使用 Windows 10 内置 Segoe MDL2 Assets 媒体图标字体，不引入 Cargo 依赖、图片包或外部框架。

怎么跑：
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\build-dev.cmd fmt -- --check"`
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\build-dev.cmd clippy --target-dir target-ui -- -D warnings"`
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\build-dev.cmd test --target-dir target-ui"`
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\build-dev.cmd build --target-dir target-ui"`
- `target-ui\debug\mintcat-proc-mic.exe --snapshot-ui --out %TEMP%\processmic-icons.raw`
- 同命令再加 `--menu`、`--category-menu`、`--category-row-menu` 检查菜单。

实测：
- Rust 格式、严格 Clippy、构建通过；17 项单元测试通过。
- 默认播放器、条目菜单和两类分类菜单快照通过，尺寸 1080×600；快照中播放器与添加文件按钮显示为 Windows 图标字形，无搜索框。
- `src/ui_bridge` 与 `ui/` 中音频搜索字段、回调和缓存引用已清除；分类可见索引测试改为覆盖分类及后代边界。
- 未验证真实音频播放听感及物理鼠标操作。
- 完整行数脚本仍因既有 `src/library.rs` 521 行超过 400 行失败；引擎和 capture 文件命中不增长基线，本轮未改动这些模块。

下一步：
- 用户要求的“添加文件夹”操作在当前代码中不存在；若需要文件夹导入，应另行实现原生目录选择和批量导入，而不是把单文件选择按钮改名伪装。

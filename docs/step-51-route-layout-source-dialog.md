# Step 51 — 路由卡片布局与添加源对话框
范围：修复路由页源卡片区域的宽度/滚动布局，移除底部添加卡片；顶部添加源按钮改为页面内小型对话框；压缩状态栏总音量控件间隔并延长滑块。未改变音频引擎、配置模型和音频播放语义。

改动文件：
- `ui/pages/route.slint`：源卡片集合明确使用全宽 `ScrollView`，删除底部添加卡片，接入顶部对话框。
- `ui/components/add-source-dialog.slint`：新增添加源对话框，支持添加进程、导入文件、麦克风混入和进入音频库。
- `ui/components/page-host.slint`、`ui/app-window.slint`：接通从对话框进入音频库的页面回调。
- `ui/components/status-bar.slint`：总音量改用无隐藏标签的直接滑块，减少空隙并增大有效轨道。
- `FRONTEND.md`、`ARCHITECTURE.md`、`docs/step-02-rust-architecture.md`、`PROJECT.md`：同步当前 UI 边界和布局事实。

怎么跑：
```bat
scripts\build-dev.cmd build
target\debug\mintcat-proc-mic.exe --snapshot-ui --page route --out route-layout.raw
```

本轮实际使用的独立目标目录命令（避免已有运行中的 `target\debug\mintcat-proc-mic.exe` 锁文件）：
```bat
cmd.exe /c "set CARGO_TARGET_DIR=target\verify-layout&& scripts\build-dev.cmd build"
target\verify-layout\debug\mintcat-proc-mic.exe --snapshot-ui --page route --out target\verify-layout\route-layout.raw
```

实测：2026-10-03，Windows x64。
- 默认目标目录构建首次因运行中的旧 exe 被 Windows 锁定，返回 `os error 5`；未结束用户进程，改用独立 `CARGO_TARGET_DIR`。
- 独立目标目录 `scripts\build-dev.cmd build` 成功，Slint 页面与 Rust 组合根编译通过。
- `--snapshot-ui --page route` 成功输出 `1280x800` RGBA 快照；视觉核对确认源卡片横向占满内容区，底部添加卡片不再出现，状态栏滑块有效轨道明显变长。
- 添加源对话框通过同一 Slint 构建验证；原生窗口点击/打开对话框仍需桌面人工验收。

下一步：手工点击顶部“添加音频源”，分别验证添加进程、进入音频库后使用“添加到路由主页”、导入文件和麦克风混入；验证多源超出窗口高度时 ScrollView 滚动。

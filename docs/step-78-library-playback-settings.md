# Step 78 — 音频库播放设置收口到路由
范围：删除音频详情中的播放按钮、本机试听按钮、输出设备下拉和文件音量滑块；音频库详情保留文件元数据、分类和快捷键管理。路由页继续负责音频源的输出目标与路由增益。修复历史 `file_output_id` 导致未使用的 VB-CABLE 端点被打开的问题。

改动文件：
- `ui/components/audio-detail.slint`：移除播放/试听/详情上下文按钮，以及输出设备和文件音量控件及其回调契约。
- `ui/pages/player.slint`、`ui/components/page-host.slint`：移除音频详情对应的播放设置属性、事件和失效绑定。
- `src/ui_bridge/callbacks/route_start.rs`：文件槽位没有路由卡片级输出目标时回退主输出，不再读取历史全局 `file_output_id`。
- `src/ui_bridge/callbacks/route_start/source_only.rs`：单独转发文件使用对应路由卡片的输出目标。
- `src/ui_bridge/sources.rs`：路由文件卡片默认输出和添加源窗口默认输出跟随主输出，不被旧文件输出配置污染。
- `FRONTEND.md`、`ARCHITECTURE.md`、`BACKEND.md`、`DEBUGGING.md`、`PROJECT.md`：同步音频库职责和设备忙排障规则。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\build-dev.cmd build
scripts\build-dev.cmd test
scripts\check.cmd
scripts\smoke.cmd
```

实测：2026-10-19，Windows x64。
- `scripts\build-dev.cmd fmt -- --check` 成功。
- `scripts\build-dev.cmd build` 成功。
- `scripts\build-dev.cmd test`：34 passed、0 failed。
- `scripts\check.cmd` 成功；仅有既有非阻断行数审查提示：`src/ui_bridge/callbacks/route/sources.rs` 650 行、`ui/app-window.slint` 471 行、`ui/components/add-source-panel.slint` 493 行。
- `scripts\smoke.cmd` 成功，路由页、音频库页和添加源窗口快照生成成功。
- 用户报告的设备忙日志作为本轮根因依据；未重复运行用户已经提供的真实设备失败场景。真实 VB-CABLE 多端点听感和桌面点击仍需手工验收。

下一步：在实际配置下确认路由输出选择为「CABLE虚拟扬声器」时，启动路由不会再因历史音频库文件输出配置打开「CABLE In 16ch」。

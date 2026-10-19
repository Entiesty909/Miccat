# Step 82 — 路由音频控制与添加源布局

范围：修复 `sources.rs` 已生成但未显示的音频状态；让路由音频卡片的播放/暂停/停止按各自 SFX 槽位工作；移除添加音频源的音频/麦克风双栏外层底色卡片；修复麦克风设备行高度与顶端排列，并以输出设备下拉替换旧设备设置尾部占位内容。麦克风输出选择写入既有 `AppConfig.output.mic_output_id`，默认项跟随路由主输出。

改动文件：
- `src/audio/sfx.rs`、`src/engine.rs`、`src/engine/output.rs`、`src/engine/playback.rs`：追踪 SFX 槽位状态，支持暂停/恢复和停止后从头播放。
- `src/ui_bridge/sources.rs`、`src/ui_bridge/callbacks/route/sources.rs`：按槽位生成状态，更新行状态并接通卡片控制。
- `ui/components/source-row.slint`：显示状态并将播放/暂停按钮绑定到该卡片状态。
- `ui/components/add-source-panel.slint`、`ui/components/audio-source-list.slint`、`ui/components/audio-source-details.slint`、`ui/windows.slint`：调整双栏卡片、麦克风列表布局和输出设备选择。
- `src/ui_bridge/callbacks/route.rs`、`src/ui_bridge/mod.rs`、`src/ui_bridge/status.rs`、`src/ui_bridge/dev/snapshot.rs`、`src/cli/mod.rs`：同步输出选项、回调、快照及 SFX 探针。
- `ARCHITECTURE.md`、`BACKEND.md`、`API.md`、`FRONTEND.md`、`DEBUGGING.md`、`docs/step-02-rust-architecture.md`、`PROJECT.md`：同步状态和操作契约。

怎么跑：
- `scripts\build-dev.cmd fmt -- --check`
- `scripts\check.cmd`
- `scripts\build-dev.cmd test`
- `scripts\smoke.cmd`
- `target\debug\miccat.exe --snapshot-ui --page route --out <file>`

实测：2026-10-20，Windows x64。格式检查、`scripts\check.cmd`、35 项单元测试和冒烟脚本通过；麦克风与音频 Tab 快照已渲染核对，列表完整靠顶、设置控件靠顶，两个独立白色卡片之间无外层白底。路由快照显示音频状态文字。单元测试验证暂停期间不消费流数据、恢复后继续输出缓冲样本。未做真实 render 设备的听感验收或原生窗口鼠标点击验收。

下一步：需要在真实转发设备上手工确认卡片启动、暂停、停止后的听感；UI 快照与 SFX 单测不能替代设备端到端验收。

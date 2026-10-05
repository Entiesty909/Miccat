# Step 52 — 路由源逻辑与源操作菜单
范围：修复路由源卡片的状态和操作语义。进程关闭保留卡片，文件关闭/打开保留暂停位置；源卡片压缩冗余文本并扩大音量滑轨；右侧更多菜单支持删除源、音频文件快捷键、定位音频库和单独转发。未改变 AudioEngine 的实时音频格式与输出线程模型。

改动文件：
- `src/ui_bridge/cache.rs`：增加进程禁用、文件源隐藏和麦克风源隐藏的桥接视图状态。
- `src/ui_bridge/sources.rs`：投影状态不再重复显示类型/状态文本，并保留关闭源的卡片状态。
- `src/ui_bridge/callbacks/route/sources.rs`：修复进程切换不再删除卡片；文件开关改为暂停/恢复；接入源删除、快捷键、定位和单独转发动作。
- `src/ui_bridge/callbacks/route_start.rs`、`route_start/source_only.rs`：单独转发按源类型打开对应输出链路。
- `src/engine.rs`：增加按启用进程列表重建捕获线程的安全 API，删除不再使用的单槽替换实现。
- `ui/components/source-row.slint`、`slider-row.slint`、`source-actions-menu.slint`：紧凑源卡片、宽音量滑轨和源操作菜单。
- `ui/pages/route.slint`、`ui/components/page-host.slint`、`ui/app-window.slint`：透传带动作码的源菜单回调。

怎么跑：
```bat
cmd.exe /c "set CARGO_TARGET_DIR=target\verify-route-logic&& scripts\build-dev.cmd build"
target\verify-route-logic\debug\mintcat-proc-mic.exe --snapshot-ui --page route --out target\verify-route-logic\route-logic.raw
cmd.exe /c "set CARGO_TARGET_DIR=target\verify-route-logic&& scripts\build-dev.cmd clippy -- -D warnings"
cmd.exe /c "set CARGO_TARGET_DIR=target\verify-route-logic&& scripts\build-dev.cmd test"
```

实测：2026-10-03，Windows x64。
- 独立目标目录构建成功。
- 严格 Clippy `-D warnings` 成功。
- 单元测试 21 passed、0 failed。
- 路由页面快照成功输出 1280x800；源卡片内容紧凑，操作组保持横向，音量滑轨获得剩余宽度，未出现控件重叠。
- 行数门禁通过；历史超限文件 `src/audio/capture.rs` 与 `ui/app-window.slint` 未增长。
- 真实桌面点击、音频设备听感和单独转发的实际输出仍需 Windows 桌面/真实设备手工验收；本轮未宣称这些路径已自动化证明。

下一步：手工验证进程开关关闭/打开、文件暂停/恢复、更多菜单删除和单独转发；确认删除文件源不会删除音频库文件，单独转发只启动选中的源。

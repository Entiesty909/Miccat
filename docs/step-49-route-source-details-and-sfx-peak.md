# Step 49 — 路由音源与音频库交互收口

范围：同一轮收口真实音频源数据、路由行、添加源入口、声纹和随后的 UI 回归。添加源音频页使用真实条目元数据；路由行显示真实进程图标、麦克风设备名及峰值；分类树、音频库编辑和源操作菜单接回现有配置/桥接路径。补齐进程/音频/麦克风入口，并修复类型卡、文件启用、菜单命中、图标编辑和状态单位等回归。不注入示例进程、音频、设备或快捷键，不改变磁盘文件名。

改动文件：
- `ui/components/{add-source-dialog,audio-source-list,audio-source-details,category-sidebar,library-splitter,status-bar,source-row,page-host}.slint`、`ui/pages/{route,player}.slint`、`ui/app-window.slint`：真实数据展示、源类型卡和独立菜单交互。
- `src/ui_bridge/{cache,library,sources,status}.rs`、`src/ui_bridge/callbacks/{library,library_entries,route,route/sources,route_start}.rs`、`src/ui_bridge/state.rs`、`src/ui_bridge/dev/snapshot.rs`：路由模型、分类编辑、刷新和静态/动态模型更新。
- `src/engine.rs`、`src/engine/{output,fanout,status}.rs`、`src/audio/{render,sfx,preview}.rs`：SFX 峰值采样与桥接读取。
- `src/platform/icon.rs`、`src/cli/mod.rs`、`Cargo.toml`、`Cargo.lock`、`scripts/smoke.cmd` 及相关契约文档。

实现与最终语义：
- 音频源目录来自 `AppConfig.audio_entries`；显示格式、时长、大小、分类、快捷键和循环状态，不用示例数据。文件图标只解码图像扩展名，其它资源走 Windows Shell。
- 分类图标编辑同名可提交，真正的同级重名仍拒绝；分隔条按拖动起点差值计算，不读变化中的绝对坐标。
- 三类添加源使用一致类型卡；麦克风下拉宽度受限，进程列表用真实 exe 图标和 PID。
- 源峰值由音频线程写 `AtomicU32`，UI 50ms 轮询只更新变化单元；峰值标 dBFS，总线增益 dB 与之分开。
- 文件源启用不启动全局路由；停止路由时不能触发文件暂停。源菜单使用 PopupWindow，脱离卡片/ScrollView 裁剪。
- 文件图标菜单按当前可见路径索引操作；进程/音频/麦克风的添加、替换和菜单动作保持各自槽位。

怎么跑：
- `scripts\build-dev.cmd fmt`
- `scripts\check.cmd`
- `scripts\build-dev.cmd test`
- `scripts\smoke.cmd`
- `miccat.exe --snapshot-ui --page route --source-audio-dialog --out "%TEMP%\miccat-source-audio.raw"`
- `miccat.exe --snapshot-ui --page route --source-dialog-click-tab process|audio|microphone --out "%TEMP%\miccat-source-tabs.raw"`
- `miccat.exe --snapshot-ui --page route --source-menu-click --out "%TEMP%\miccat-source-menu.raw"`

实测：2026-10-15，Windows x64。
- 连续实现/回归后的最终门禁通过，26 项单元测试通过；此前相邻补丁分别记录过 23、24 项测试，最终记录以最后一次 26 项结果为准。
- Smoke 与 1280×800 软件渲染快照通过，覆盖真实源模型、三种添加源页、类型卡点击切换、空分类/分类行菜单和路由源 PopupWindow。
- `ui/app-window.slint`、`ui/components/add-source-dialog.slint` 有超过常规目标的非阻断行数提示，未超过硬门禁。
- 未声称完成真实路由听感、设备失效、物理鼠标拖动、图标文件写入和用户桌面验收。

下一步：按独立验收目标在真实设备验证文件路由声音、源卡启停与麦克风替换；同批 UI 补充继续更新本记录，不另建阶段文件。

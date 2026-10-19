# Step 70 — 音频源交互与音频库细节修复
范围：修复音频库分隔条拖动、分类布局、图标编辑提交、添加音频源对话框、状态栏实时声纹、路由源分贝显示和源操作菜单；补齐进程/音频文件/麦克风加入路由及文件源启动路径。未写入示例进程、音频文件、麦克风或快捷键数据；未改变物理音频文件名。

改动文件：

- `ui/components/library-splitter.slint`
- `ui/pages/player.slint`
- `ui/components/category-sidebar.slint`
- `ui/components/audio-library-edit-dialog.slint`
- `ui/components/add-source-dialog.slint`
- `ui/components/status-bar.slint`
- `ui/components/source-row.slint`
- `ui/pages/route.slint`
- `ui/app-window.slint`
- `src/ui_bridge/cache.rs`
- `src/ui_bridge/library.rs`
- `src/ui_bridge/sources.rs`
- `src/ui_bridge/status.rs`
- `src/ui_bridge/callbacks/library.rs`
- `src/ui_bridge/callbacks/library_entries.rs`
- `src/ui_bridge/callbacks/route.rs`
- `src/ui_bridge/callbacks/route/sources.rs`
- `src/ui_bridge/callbacks/route_start.rs`

实现：

- 分隔条改用按下点差值，不再读取变化中的 `absolute-position`；右侧分隔条使用反向增益，使向右拖动扩大中间列表。
- 分类行改为单一 `HorizontalLayout`，图标、名称和计数不再重叠；名称显式左对齐。
- 图标编辑只保留默认音符、首字单字和自定义文件三种语义；提交前拆开缓存/config 借用，分类图标迁移和音频条目保存不再嵌套可变借用。
- 添加音频源进程页使用设计稿式三张类型卡，音频文件和麦克风页使用顶部类型标签；类型卡点击区域覆盖整卡，列表仍来自实时进程、配置音频条目和设备枚举。
- 进程选择行只显示居中的选择圆点、Windows 真实进程图标、进程名和 PID；图标在列表刷新时生成，不在 50 ms 轮询中提取。
- 状态栏源数量文本和箭头合并为紧凑触发区；Popover 固定显示在触发文字上方，源行和总线均显示 32 点声纹与实时峰值 dB。
- 路由源模型增加仅更新电平/状态的 50 ms 动态路径，避免每次轮询重建图标、下拉和 Repeater；源菜单提升到路由页顶层，脱离 `ScrollView` 裁剪。
- 新增 `route_audio_entry` 视图状态；无进程时也可用音频文件或麦克风启动路由。路由启动在输出打开后投递文件源，音频卡片的启动按钮在路由未运行时启动完整路由。

怎么跑：

```bat
scripts\build-dev.cmd fmt
scripts\build-dev.cmd build
scripts\build-dev.cmd test
scripts\check.cmd
scripts\smoke.cmd
```

实测：2026-10-15，Windows x64。

- Debug 构建成功。
- 24 个单元测试通过。
- `scripts\check.cmd` 通过；`ui/app-window.slint` 469 行、`ui/components/add-source-dialog.slint` 532 行为非阻断审查提示，硬上限通过。
- `scripts\smoke.cmd` 成功枚举本机 7 个音频端点、10 个进程，并生成 7 张 1280×800 软件渲染快照，覆盖主界面、菜单、空分类/分类菜单和进程/音频/麦克风添加源页。
- 额外生成路由页添加源快照 `miccat-route-final.raw`，软件渲染成功。
- 真实鼠标拖动分隔条、编辑对话框自定义图标选择/保存、Popover 点击、真实路由声音和设备失效路径仍需 Windows 桌面及音频设备手工验收；本轮未伪造这些结果。

性能：

- 实时音频线程未增加 UI、文件、锁或分配操作。
- 50 ms UI 轮询只对变化的源行模型单元写回；静态源图标和下拉模型不在轮询中重建。
- 总线/源峰值仍由音频线程原子采样，UI 只消费峰值并维护固定 32 点历史。

下一步：手工验证分隔条拖动、三种源加入/启动、编辑图标保存、状态栏 Popover 定位和真实虚拟设备声音。

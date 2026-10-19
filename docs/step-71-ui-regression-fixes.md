# Step 71 — UI 回归与源路由语义修复
范围：统一添加音频源的进程/音频文件/麦克风类型卡；修复文件源开关误启动全局路由、分类图标编辑同名保存被拒绝、路由源菜单被卡片/滚动区限制等回归；明确声纹峰值 dBFS 与总线增益 dB 的含义。未引入设计样例进程/音频/麦克风数据，未引入依赖。

改动文件：

- `ui/components/add-source-dialog.slint`
- `ui/components/source-row.slint`
- `ui/components/status-bar.slint`
- `ui/pages/route.slint`
- `ui/app-window.slint`
- `ui/components/page-host.slint`
- `src/ui_bridge/state.rs`
- `src/ui_bridge/lists.rs`
- `src/ui_bridge/status.rs`
- `src/ui_bridge/callbacks/route/refresh.rs`
- `src/ui_bridge/cache.rs`
- `src/ui_bridge/sources.rs`
- `src/ui_bridge/callbacks/route/sources.rs`
- `src/ui_bridge/callbacks/route_start.rs`
- `src/ui_bridge/callbacks/library.rs`
- `src/ui_bridge/library.rs`
- `src/ui_bridge/dev/snapshot.rs`
- `src/cli/mod.rs`
- `scripts/smoke.cmd`
- `ARCHITECTURE.md`、`BACKEND.md`、`FRONTEND.md`、`DEBUGGING.md`、`API.md`、`PROJECT.md`

修复：

- 进程/音频文件/麦克风始终使用同一组三张完整类型卡；图标、标题、说明、尺寸、点击区域一致，选中时只切换背景和边框。
- 新增 `--source-dialog-click-tab process|audio|microphone`：从另一Tab开始，向卡片发送 Slint 左键事件并断言 `selected-tab` 已切换；`scripts/smoke.cmd` 覆盖三种目标。
- 路由麦克风源的下拉框限制为 190px，左边与源卡名称列对齐；进程列表删除未要求的“正在出声/未出声”模型，只保留选择圆点、真实图标、名称和 PID。
- `UiCache.route_file_enabled` 与文件播放、全局 route lifecycle 分离。路由未运行时切换文件源只记录启用状态，不调用全局启动；全局转发只由状态栏“开始转发”控制。启动时仅投递启用的文件源。
- 路由未运行时禁用文件行播放/暂停按钮；播放回调只对正在运行且正在播放的文件发出暂停操作。
- 分类图标编辑允许分类名不变；图片/首字更新写入 `audio_category_icons` 并调度 `SaveQueue`。新增回归测试覆盖同名图标编辑放行及真正同级重名仍拒绝。
- 路由更多菜单由独立 Slint `PopupWindow` 承载，锚定 `...` 按钮下方并钳制在页面内；点击外部关闭，菜单内部点击只在选中菜单项/空白处显式关闭，不被 `ScrollView` 裁剪。
- 声纹标题明确为“峰值 dBFS”；底部显示“总线增益（相对 100%）”。dBFS 是数字信号相对满刻度的峰值，增益 dB 是相对 100% 音量倍率的对数表示，二者不是同一个量。

验证：

```bat
scripts\check.cmd
scripts\build-dev.cmd test
scripts\smoke.cmd
```

实测：2026-10-15，Windows x64。

- 完整门禁通过。
- 26 个单元测试通过，包含“分类名称未变仍可编辑图标”及“同级重名仍拒绝”回归测试。
- `scripts/smoke.cmd` 生成三类真实源页快照，模拟三种类型卡左键并断言切换；另点击路由首条源的 `...` 按钮，生成 PopupWindow 菜单快照。
- 添加源快照为 1280×800；渲染显示三张类型卡始终同一布局，音频文件卡选中态正确。路由快照显示菜单浮在卡片上方、锚定在 `...` 下方且未被裁剪。
- 非阻断行数审查提示：`ui/app-window.slint` 467 行、`ui/components/add-source-dialog.slint` 531 行；硬门禁通过。
- 尚需桌面/设备手工验收：确认麦克风下拉实际宽度/左边对齐；文件源启用不改变全局转发状态；启动后检查文件输出；自定义 PNG/ICO 与首字分类图标保存后重开确认；点击菜单项与外部失焦关闭。本轮不声称这些物理鼠标/真实音频路径已手工验收。

性能：

- 音频线程无新增 UI、锁、文件或分配逻辑。
- 文件源启用状态留在桥接缓存，不启动音频线程；实时源刷新只更新变化的电平/状态模型。
- 类型卡交互不枚举进程/设备；菜单坐标只在触发时计算，不进入 50 ms 轮询。
- 进程图标在进程列表更新时提取，不随源峰值的 50 ms 轮询重复加载。
- 麦克风下拉仅限 190px，避免随左侧列横向伸展。

下一步：桌面验收文件源开关与实际转发、分类图片文件选择/保存，以及源菜单的物理点击与失焦关闭。

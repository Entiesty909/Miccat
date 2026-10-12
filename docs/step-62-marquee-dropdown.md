# Step 62 — 跑马灯下拉列表

范围：
- 做了：新增可复用 `MarqueeText`，使用 `Text.preferred-width` 实测单行文字宽度；裁剪溢出内容，悬停时缓慢往返滚动，首尾停顿，离开后复位。
- 做了：新增 `MarqueeComboBox`，提供标准字符串模型、`current-index`、`current-value` 和 `selected(string)` 接口；下拉选项使用 `ListView` 与 `MarqueeText`。
- 做了：路由主页源卡片的输出设备/物理麦克风选择，以及“添加音频源”对话框的进程选择改用该控件。
- 保持不变：`process-index` 的双向绑定、进程标签与 PID 列表顺序、`process-add` 回调和 Rust 音频路由逻辑；对话框麦克风、音频库和设置页的其他标准 `ComboBox` 未替换。

改动文件：
- `ui/components/marquee-text.slint`
- `ui/components/marquee-combo-box.slint`
- `ui/components/source-row.slint`
- `ui/components/add-source-dialog.slint`
- `FRONTEND.md`、`DEBUGGING.md`、`PROJECT.md`
- `docs/step-62-marquee-dropdown.md`

性能与生命周期：
- 文本宽度来自 Slint 实际布局测量，不按字符数估算。
- `ListView` 只实例化可见选项；跑马灯计时器只在文本溢出且对应选项悬停时运行。
- `PopupWindow` 在关闭时保留轻量宿主（Slint 1.18 不允许直接条件创建 `PopupWindow`），但 `is-open` 控制的焦点区和列表子树会卸载；列表行及其计时器随之销毁。
- 样式决策：不把全项目所有标准 `ComboBox` 强行替换为新控件；保留标准控件以降低行为回归范围，并将 `MarqueeComboBox` 调整为同样的字段底、细边框、圆角、箭头、悬停/按下底色和弹出列表内边距。重开菜单时当前选项使用浅灰选中底色；这是标准样式统一，因此不额外加入独立的特殊选中交互。

- 路由输出下拉框的已选值保留裁剪和悬停动画；其他已选值使用省略显示，避免每个常驻下拉框都持有跑马灯计时器。
- 不新增 Cargo 依赖；不改进程枚举、WASAPI、COM 或音频引擎。

怎么跑：
- `scripts\build-dev.cmd build`
- `target\debug\miccat.exe`
- 路由页点“添加音频源”检查进程下拉；源卡片检查输出设备和物理麦克风下拉。

实测：2026-10-13，Windows x64。
- `cargo check`：通过。
- `scripts\build-dev.cmd build`：通过。
- 路由、音频库和设置页的 `--snapshot-ui` 快照：均输出 1280×800。
- 用户手工验收反馈：长短文本、悬停启动/离开复位、菜单展开/收起与选择行为均正常。
- 本次未加入真实声卡或音频路由测试；控件改动不触及音频链路。

# Step 63 — 跑马灯开关与选中标识

范围：
- 做了：为 `MarqueeComboBox` 增加可绑定的 `marquee-enabled` 属性，默认开启；关闭时已选文本和下拉选项均停止跑马灯。
- 做了：左侧蓝色指示条改为只依据 `current-index` 标记实际已选项；悬停/键盘高亮状态不再控制该指示条。
- 保持不变：选中值的浅灰底、悬停底色、索引绑定和进程添加回调。
- `marquee-enabled=false` 时，闭合值使用普通 `Text`，下拉选项也使用普通 `Text`；不会创建 `MarqueeText` 的 Timer，因此没有逐项计时开销，仅保留控件本身的模型和弹窗结构。
- 闭合状态文字视口固定为控件宽度的 `82%`，右侧为下拉箭头和安全间距；长文字省略或在开启跑马灯时只在该视口内移动，不会覆盖箭头。

改动文件：
- `ui/components/marquee-combo-box.slint`
- `ui/components/source-row.slint`
- `FRONTEND.md`、`PROJECT.md`
- `docs/step-63-marquee-toggle-selection.md`

怎么跑：
- `scripts\build-dev.cmd build`
- `target\debug\miccat.exe --snapshot-ui --page route --out target\step-63-route.raw`

实测：2026-10-13，Windows x64。
- `cargo check`：通过。
- 常规 `scripts\build-dev.cmd build` 因运行中的 Miccat 占用 `target\debug\miccat.exe` 返回 `os error 5`；未停止现有进程，改用 `scripts\build-dev.cmd build --target-dir target\step64-build`，构建通过。
- `target\step64-build\debug\miccat.exe --snapshot-ui --page route`：生成 1280×800 路由页快照。
- 本次验证了关闭开关时组件结构可编译，未声称实际鼠标动画或每行蓝色指示条的视觉交互由我重新操作验证。
- 本阶段不改音频引擎或设备路由。

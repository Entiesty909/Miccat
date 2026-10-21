# Step 47 — 音频库页面与长文本下拉交互收口

范围：同一批收口音频库三栏内容区和路由/进程列表中的长文本下拉。真实分类、条目、进程和设备模型继续沿用现有配置/枚举；不注入设计稿样例数据，不改音频引擎。

功能要求：
1. 音频库使用分类、文件表、详情三栏；按当前分类及后代显示真实文件，保留搜索/排序/分页与元数据索引映射。
2. `MarqueeText`/`MarqueeComboBox` 用实际 `Text.preferred-width` 检测溢出；长名称仅在悬停时滚动，保持字段宽度、短文本省略和菜单定位稳定。
3. `marquee-enabled` 可绑定；关闭后选中值和下拉项停止滚动并且不创建逐行 Timer；蓝色标识只跟随 `current-index`，不跟随悬停/键盘高亮。
4. 播放器和音频库列表按真实配置布局；详情显示实际元数据，空分类/筛选显示真实空态。

改动文件：
- `ui/pages/player.slint`、`ui/components/{category-sidebar,audio-list,audio-detail,audio-transport,library-splitter,marquee-text,marquee-combo-box,source-row,add-source-panel,page-host}.slint`、`ui/app-window.slint`。
- `src/ui_bridge/{cache,library}.rs`、`src/ui_bridge/callbacks/{library_view,player}.rs`、`FRONTEND.md`、`API.md`、`DEBUGGING.md`、`PROJECT.md`、`docs/audio-library-redesign-plan.md`。

怎么跑：
- `scripts\build-dev.cmd fmt`
- `scripts\build-dev.cmd build`
- `scripts\build-dev.cmd test`
- `scripts\check.cmd`
- `target\debug\miccat.exe --snapshot-ui --page player --out "%TEMP%\miccat-library.raw"`
- `target\debug\miccat.exe --snapshot-ui --page route --out "%TEMP%\miccat-route-dropdown.raw"`

实测：2026-10-13 至 2026-10-14，Windows x64。
- Debug 构建、默认 `cargo check`、门禁和 23 项单元测试通过；音频库与路由页 1280×800 快照通过。
- 软件渲染确认三栏、真实配置条目、工具栏/详情区域和路由长文本字段存在；长文本悬停滚动行为曾由用户手工验收正常。
- 关闭跑马灯后不创建 `MarqueeText` Timer，组件结构和索引选择契约由实现/构建核对；本记录不声称重新执行了所有鼠标交互。
- 未新增依赖；真实音频设备听感不属于本批 UI 变更。

下一步：若修改同一列表/下拉/音频库页面，追加到本记录，不为小范围视觉或 bug 修复另建阶段文档。

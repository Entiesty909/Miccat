# Step 52 — 路由卡片布局与滚动条留白收口

范围：同一批调整路由源卡片紧凑布局、真实进程图标、启用区对齐和垂直滚动条避让。不改变源类型、卡片顺序或滚动行为；`resources/icon.ico` 仅用于 Windows 窗口标题和 exe 资源。

改动文件：
- `ui/components/source-row.slint`：紧凑列布局、图文间距、启用开关与竖分隔线对齐；进程显示对应 exe 图标。
- `ui/pages/route.slint`：源列表右侧预留 18px，避开 Fluent ScrollView 内侧 14px 滚动条绘制区。
- `ui/components/add-source-panel.slint`：进程选择项显示真实 exe 图标。
- `ui/app-window.slint`、`ui/components/app-header.slint`、`resources/icon.ico`、`resources/app.rc`、`build.rs`：应用 ICO 绑定 Windows 主窗和 exe 资源；品牌操作栏不再放应用图标。
- `FRONTEND.md`、`PROJECT.md`：同步卡片列和图标位置契约。

怎么跑：
- `scripts\build-dev.cmd fmt -- --check`
- `scripts\build-dev.cmd test`
- `scripts\check.cmd`
- `scripts\build-dev.cmd build`
- `target\verify-route-icons\debug\miccat.exe --snapshot-ui --page route --source-meter --out "%TEMP%\miccat-route-cards.raw"`

实测：2026-10-19，Windows x64。
- Debug 构建、格式检查、`scripts\check.cmd` 和 34 项单元测试通过。
- 路由 1280×800 RGBA 快照确认卡片可见区域及真实进程图标；系统主窗/exe 图标使用 `resources/icon.ico`。
- Fluent ScrollView 竖条宽 14px，列表右侧预留 18px，几何上保证额外 4px 间距；快照只有三行，没有可见滚动条，长列表下的真实桌面间距仍需验收。
- 未确认根目录重复 ICO 删除，因为文件锁未解除；未声称完成真实桌面拖动与长列表滚动验收。

下一步：在较长路由源列表中确认滚动条与卡片右边缘保持间距；同批卡片布局补丁更新本记录。

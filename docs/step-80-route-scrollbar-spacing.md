# Step 80 — 路由卡片避让垂直滚动条
范围：修复路由页源卡片与 Fluent `ScrollView` 垂直滚动条重叠。滚动容器的竖滚动条在 viewport 内侧覆盖 14px；源行列表右侧加入 18px 预留，确保卡片边框和最右侧启用控件不进入滚动条绘制区域。未改滚动行为和卡片列顺序。

改动文件：
- `ui/pages/route.slint`：源行 `VerticalLayout` 增加 18px 右侧 padding。
- `FRONTEND.md`：记录滚动条叠加区域与列表预留宽度。

怎么跑：
```bat
set CARGO_TARGET_DIR=target\verify-route-icons
scripts\build-dev.cmd fmt -- --check
scripts\build-dev.cmd build
scripts\build-dev.cmd test
target\verify-route-icons\debug\miccat.exe --snapshot-ui --page route --source-meter --out target\verify-route-icons\route-scrollbar.raw
```

实测：2026-10-19，Windows x64。
- 格式检查通过。
- 独立目标目录构建成功。
- 34 个单元测试通过。
- 路由页 1280×800 RGBA 快照生成成功；复查 Fluent ScrollView 实现确认竖滚动条宽 14px、x 坐标位于 viewport 内侧，18px 列表留白覆盖该绘制区并额外留 4px 间隙。
- 此快照当前只有三条源行，不会出现可见滚动条；卡片间距修复依据 Fluent 控件的实际绘制宽度实现，真实长列表滚动仍需在桌面确认。

下一步：用户在路由源数量超过可视区域时确认滚动条与最右卡片边框/启用控件之间保持空隙。

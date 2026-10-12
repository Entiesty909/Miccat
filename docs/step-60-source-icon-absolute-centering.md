# Step 60 — 源图标改为卡片绝对居中
范围：修正上轮布局居中仍无效的问题。原因是图标外层仍是横向布局子项，布局引擎按交叉轴起点放置它；本轮将图标块移出 HorizontalLayout，直接相对 SourceRow 使用 `y: (root.height - self.height) / 2` 定位。横向布局内保留透明 56px 占位，不改变名称、设备、音量和操作列位置。

改动文件：
- `ui/components/source-row.slint`：图标块脱离横向布局，按整张卡片高度绝对居中；真实图标和默认 glyph 仍在 56px 容器内居中。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\check.cmd
scripts\smoke.cmd
miccat.exe --snapshot-ui --page route --out target\miccat-icon-absolute.raw
```

实测：2026-10-11，Windows x64。
- `scripts\check.cmd` 通过；既有超限基线文件未增长。
- `scripts\smoke.cmd` 通过。
- 路由快照 1280x800：进程和音频源的图标块均位于各自卡片上下中心，不再贴近卡片顶部。

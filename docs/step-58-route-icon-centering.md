# Step 58 — 路由卡片图标垂直居中
范围：修正路由卡片图标容器在卡片内的垂直位置。图标容器现在由占满卡片高度的 VerticalLayout 居中布局，进程 exe 图标、音频文件图标和麦克风图标的 56px 背景块上下居中；不改变图标提取和默认回退逻辑。

改动文件：
- `ui/components/source-row.slint`：用垂直居中的布局包裹源图标容器，图片自身继续按容器尺寸居中。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\check.cmd
scripts\smoke.cmd
miccat.exe --snapshot-ui --page route --out target\miccat-route-buttons.raw
```

实测：2026-10-11，Windows x64。
- 格式检查通过。
- `scripts\check.cmd` 通过；既有超限基线文件未增长。
- `scripts\smoke.cmd` 通过。
- 路由快照 1280x800：源图标背景块在各自卡片内上下居中；“添加音频源”和“刷新进程”并列显示。

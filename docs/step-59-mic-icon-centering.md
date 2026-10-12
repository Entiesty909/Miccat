# Step 59 — 麦克风卡片图标相对整卡片居中
范围：根据用户提供的红框截图修正麦克风源卡片图标块位置。此前图标块只在内容布局高度内居中，麦克风下拉框使内容区域变高后，图标块视觉上偏上；现在图标布局明确占满 `root.height`，再在整张卡片高度内居中。

改动文件：
- `ui/components/source-row.slint`：图标外层 VerticalLayout 使用 `height: root.height` 和 `vertical-stretch: 1`，图标块相对整张源卡片上下居中。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\check.cmd
miccat.exe --snapshot-ui --page route --out target\miccat-mic-center.raw
```

实测：2026-10-11，Windows x64。
- `scripts\check.cmd` 通过；既有超限基线文件未增长。
- 路由软件渲染快照成功输出 1280x800。
- 修复针对用户红框中的麦克风卡片：图标块不再按名称/麦克风下拉内容顶部对齐，而是按整张卡片高度居中。

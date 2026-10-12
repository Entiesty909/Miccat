# Step 57 — 路由页恢复添加音频源按钮
范围：恢复路由页“当前音频源”标题行右侧的“添加音频源”按钮，并与“刷新进程”按钮横向并列。按钮继续打开既有添加源对话框，不改变进程刷新和添加源回调。

改动文件：
- `ui/pages/route.slint`：在标题行右侧恢复“＋ 添加音频源”，与“刷新进程”并列。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\build-dev.cmd build
miccat.exe --snapshot-ui --page route --out target\miccat-route-buttons.raw
```

实测：2026-10-11，Windows x64。
- 格式检查通过。
- `scripts\build-dev.cmd build` 通过。
- 路由软件渲染快照成功输出 1280x800；“添加音频源”和“刷新进程”在“当前音频源”标题行右侧并列显示。

下一步：手工点击“添加音频源”，确认既有对话框打开并可继续选择进程、音频库或导入文件。

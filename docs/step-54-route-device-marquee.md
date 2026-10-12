# Step 54 — 路由设备名称滚动与菜单触发态
范围：修复路由源卡片两个细节。输出设备下拉框保持固定容器宽度，过长的选中名称只在容器内左右往返滚动；源操作菜单打开后隐藏 `...` 触发按钮，避免触发按钮与菜单同时占据右侧。未改变设备模型、输出选择回调或菜单动作码。

改动文件：
- `ui/components/source-row.slint`：在输出设备 ComboBox 上覆盖固定宽度的裁剪文本层，按当前选中值对过长名称做往返动画；菜单打开时条件隐藏 `...` 按钮。
- `FRONTEND.md`：同步输出名称滚动和菜单触发态契约。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\build-dev.cmd build
scripts\smoke.cmd
target\debug\mintcat-proc-mic.exe --snapshot-ui --page route --out target\route-detail-check.raw
```

实测：2026-10-11，Windows x64。
- `fmt -- --check` 成功。
- `scripts\build-dev.cmd build` 成功。
- `scripts\smoke.cmd` 返回成功。
- 路由软件渲染快照成功输出 1280x800；输出设备列宽保持不变，长名称在裁剪容器内滚动显示；源菜单触发按钮通过条件渲染隐藏。
- 未进行真实桌面鼠标点击和多窗口宽度手工验收；菜单动作本身沿用既有回调路径。

下一步：手工点击长名称设备下拉框，确认打开弹出列表时不被滚动文本层影响；打开和关闭 `...` 菜单确认触发按钮恢复。

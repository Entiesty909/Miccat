# Step 76 — 窗口定位与重复显示修复
范围：修复主窗口和两个独立子窗口的首次定位、位置恢复、重复显示重绘，以及添加源窗口提交后的关闭行为。未改音频路由的全局启停契约。
改动文件：`src/ui_bridge/mod.rs`、`FRONTEND.md`。

怎么跑：
- `scripts\\build-dev.cmd fmt`
- `scripts\\build-dev.cmd build`
- `scripts\\build-dev.cmd test`
- `scripts\\check.cmd`
- `scripts\\smoke.cmd`
- `target\\debug\\miccat.exe --snapshot-ui --page route --source-dialog-tab process --source-dialog-reopen --out <临时目录>\\miccat-reopen.raw`

实测：2026-10-19，Windows x64。
- Debug 构建成功。
- 32 项单元测试全部通过。
- `--source-dialog-reopen` 成功生成 698×620 快照；隐藏再显示后的像素一致性检查通过。
- `scripts\\smoke.cmd` 成功；设备、进程枚举以及主窗口、菜单、分类菜单、三种添加源页、源菜单和声纹 Popover 快照均生成成功。
- 主窗口首次显示改为先设置尺寸、再显示并立即居中；已有坐标仍在显示前恢复。独立窗口每次显示后额外在下一轮事件循环请求重绘。
- 添加进程和添加音频文件到路由后，添加源窗口保存位置并隐藏；添加动作不调用全局启停，路由仍由状态栏控制。

下一步：真实桌面鼠标操作仍需人工确认主窗口和两个子窗口的物理位置恢复，以及第二次打开分类/音频编辑窗口的实际画面；真实设备出声不属于本轮修改。

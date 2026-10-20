# Step 83 — 麦克风源添加与型号显示
范围：修复添加物理麦克风后添加源独立窗口仍可见的问题；路由麦克风卡片第一行固定显示“麦克风”，第二行灰色小字显示所选设备的 Windows 友好名称；普通“添加音频源”入口每次打开第一个“进程”Tab。未改变麦克风捕获、输出设备选择、混音和默认麦克风策略。

改动文件：
- `ui/components/add-source-panel.slint`：添加麦克风按钮改走独立提交回调；列表切换仍只改变选择。
- `ui/windows.slint`：转发麦克风提交回调。
- `src/ui_bridge/mod.rs`：提交后同步主窗口麦克风选择、启用混音并隐藏添加源窗口。
- `src/ui_bridge/sources.rs`、`src/ui_bridge/dev/snapshot.rs`：麦克风源行和软件快照统一为第一行“麦克风”、副标题显示实际设备名称。
- `ui/components/source-actions-menu.slint`、`src/ui_bridge/callbacks/route/sources.rs`：增加麦克风“替换麦克风”菜单动作，复用单标签添加源窗口。
- `src/ui_bridge/callbacks/route.rs`：普通添加源入口清除替换状态并重置为第一个进程 Tab。
- `src/config/mod.rs`、`src/ui_bridge/persist.rs`：保存并恢复进程卡片禁用槽位、音频卡片路径和启用状态。
- `scripts\\build-dev.cmd fmt -- --check`：预期格式检查成功。
- `scripts\\build-dev.cmd test`：预期全部单元测试通过。
- `scripts\\check.cmd`、`scripts\\smoke.cmd`：预期门禁和设备/进程、添加源三标签页快照成功。
- `target\\debug\\miccat.exe --snapshot-ui --page route --source-dialog-tab microphone --out <file>`：预期生成麦克风添加源窗口 RGBA 快照。

实测：2026-10-20，Windows x64。
- Debug 构建成功。
- Rust 格式检查成功。
- `scripts\\check.cmd` 成功；行数仅报告既有审查阈值警告，门禁通过。
- 35 项单元测试通过。
- `scripts\\smoke.cmd` 成功，生成麦克风添加源窗口、路由页和源菜单相关快照；设备枚举包含真实 Windows 音频设备名称。
- 麦克风添加窗口快照成功生成（698×620）；路由页快照成功生成（1280×800）。
- 真实鼠标点击“添加麦克风”、替换麦克风和关闭软件后重开卡片状态仍需桌面手工操作；本轮未冒称已完成这些路径。

下一步：手工选择物理麦克风，点击“添加麦克风”，确认窗口隐藏、路由卡片显示完整 Windows 友好名称，再执行开始/停止转发听感验收。

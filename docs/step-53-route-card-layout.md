# Step 53 — 路由源卡片布局与操作区
范围：按确认的主页预览重排路由源卡片、源操作菜单和底部状态栏；隐藏状态胶囊标签，统一卡片高度与字段对齐，保留现有音频行为。麦克风源下拉沿用物理捕获设备列表。未实现 exe 图片提取或新增图片依赖；源图标继续使用现有 Windows MDL2 类型图标。

改动文件：
- `ui/components/source-row.slint`：统一 112px 卡片高度；名称、类型、PID/麦克风选择、输出设备、音量列对齐；启用开关与操作按钮分行，`...` 固定最右，删除重复百分比。
- `ui/components/source-actions-menu.slint`：菜单按源类型显示单独转发、音频库定位、设置快捷键和删除源，不再展示禁用的伪操作。
- `ui/components/status-bar.slint`：将总音量、诊断和启停按钮收拢为右侧控制组，延长总音量滑轨。
- `ui/pages/route.slint`：绑定源卡片麦克风下标，使卡片下拉选择回写全局物理麦克风选择。
- `src/ui_bridge/sources.rs`：补充进程、音频文件和真实麦克风的类型/详情文案，保持既有状态与路由数据。
- `FRONTEND.md`：同步源卡片和操作区契约。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\build-dev.cmd build
scripts\smoke.cmd
target\debug\mintcat-proc-mic.exe --snapshot-ui --page route --out target\route-ui-check.raw
```

实测：2026-10-11，Windows x64。
- `fmt -- --check` 成功。
- `scripts\build-dev.cmd build` 成功。
- `scripts\smoke.cmd` 返回成功。
- 路由软件渲染快照成功输出 1280x800；卡片列对齐，麦克风下拉显示物理设备，文件源暂停/停止/更多按钮未重叠，底部总音量滑轨可见。
- 未进行真实桌面鼠标点击、真实音频输出听感或 exe 文件图标提取验证。

下一步：手工点击确认 `...` 菜单的定位、快捷键、单独转发和删除动作；确认不同窗口宽度下操作区仍不挤压。

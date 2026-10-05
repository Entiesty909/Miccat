# Step 41 — UI/UX Windows 10 视觉重构
范围：按《重构ui.md》重排 UI 视觉与页面布局；不改音频路由、设备枚举、配置持久化、播放和桥接回调契约。未实现 ListView 虚拟化、通用列表行组件和未接线的空态组件。

改动文件：
- `ui/theme.slint`：统一 Windows 10 蓝色、状态色、扁平边框、2px 圆角、间距和控件尺寸。
- `ui/app-window.slint`：重排应用顶栏、文字页签、页面容器和常驻状态栏；窗口最小尺寸调整为 900×600。
- `ui/components/button.slint`、`card.slint`、`menu-item.slint`、`tab-bar.slint`：统一默认、悬停、按下和禁用状态，去除胶囊页签。
- `ui/pages/route.slint`：重排声音来源、输出设备、进程/麦克风音量和电平信息。
- `ui/pages/player.slint`：重排工具栏、分类侧栏、音频列表和播放器；音频列表加入 `ScrollView`。
- `ui/pages/settings.slint`：按输出设备、默认设备、麦克风、虚拟线缆、热键、预览和关于分组。
- `src/ui_bridge/dev/snapshot.rs`：更新分类行快照的点击坐标以匹配新的页面布局。
- `FRONTEND.md`、`docs/refactor-plan.md`：同步 UI 契约、滚动能力和未完成项。

怎么跑：
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\build-dev.cmd build"`
- `cmd.exe /c "D:\code\MintcatProcMic\scripts\smoke.cmd"`

实测：
- 构建成功，Slint 编译通过。
- 冒烟成功：设备枚举、进程枚举、默认 UI 快照、条目菜单、空分类菜单、分类行菜单均通过；快照为 1080×600。
- 实际快照核对：顶栏为文字页签和蓝色底部指示线；音频库为紧凑分类/列表/播放器/状态栏布局，分类行菜单正常显示。
- 未自动验证：真实设备有声、物理鼠标桌面路径、路由页和设置页的实机截图、试听听感、拖动进度和停止延迟。

下一步：
- 若音频库规模需要继续扩展，再单独实现 `ListView` 虚拟化和通用列表行组件；不在本轮改变业务契约。

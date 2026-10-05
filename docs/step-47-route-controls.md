# Step 47 — 路由页控件接线审查

范围：逐项检查路由页、音源行、设置页共用刷新和底栏交互；补齐缺少的列表刷新回调，修复多进程行目标错误、音频文件菜单目标错误和混麦选择持久化。未改音频引擎路由算法。
改动文件：`ui/pages/route.slint`、`ui/components/source-row.slint`、`ui/app-window.slint`、`ui/components/page-host.slint`、`src/ui_bridge/callbacks/route.rs`、`src/ui_bridge/callbacks/route/refresh.rs`、`src/ui_bridge/callbacks/route/tests.rs`、`src/ui_bridge/callbacks/route_start.rs`、`src/ui_bridge/state.rs`、`src/ui_bridge/mod.rs`、`src/ui_bridge/persist.rs`、`src/ui_bridge/status.rs`、`src/ui_bridge/lists.rs`、`src/ui_bridge/sources.rs`、`FRONTEND.md`、`DEBUGGING.md`、`ARCHITECTURE.md`、`PROJECT.md`。
行为：路由/设置刷新在后台枚举进程、物理麦和输出端点，UI 线程一次应用结果并防止并发刷新；设备分类开关复用该异步路径。进程源行按可见行映射到确切 PID，添加/移除名称防抖持久化；麦克风选择和混入状态保存所选设备 ID，未选设备时自动关闭混入；文件源更多按钮定位实际播放路径对应的当前可见条目，文件不在当前页时不误操作其他条目；麦克风更多按钮打开音频设备设置，移除与进程开关重复的更多按钮。输出、音量、静音、文件暂停/停止、底栏音量与路由启停使用现有引擎/配置回调。
怎么跑：`scripts\build-dev.cmd fmt -- --check`、`scripts\build-dev.cmd test --target-dir target-ui`、`scripts\build-dev.cmd clippy --target-dir target-ui -- -D warnings`、`scripts\build-dev.cmd build --target-dir target-ui`、`scripts\smoke.cmd`、`--snapshot-ui --page route --out <file>`。
实测：2026-09-29，Windows x64；21 项测试通过，严格 Clippy、构建、设备/进程枚举与路由快照通过；`ui/app-window.slint` 保持 383 行历史基线。索引测试覆盖多进程行、文件/麦克风行、越界行、已退出进程过滤和音频文件当前页定位。真实桌面鼠标操作及真实虚拟设备听感未由自动化验收。
下一步：在 Windows 桌面逐项操作刷新、添加/移除进程、选麦克风、混麦、输出、音量、静音、文件控制和底栏启停，确认系统端点听感。

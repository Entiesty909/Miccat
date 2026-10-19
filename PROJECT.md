# PROJECT.md — Miccat

## 项目定位

Miccat 是 Windows 桌面进程音频路由器：以 WASAPI Process Loopback 捕获指定进程树，
把进程声与可选物理麦混音后写入一个或多个播放端点。另提供本地音频库和本机试听。

产品目标、禁止项和每轮验收以 [`goal.md`](goal.md) 为准。不是 Web 服务，也没有 HTTP API、数据库、账户或网络业务后端。

## 技术栈

- Rust 2024，单 crate。
- Slint 桌面 UI，software renderer。
- `windows` / `windows-core` 0.62、WASAPI、Media Foundation、Win32/COM。
- `serde` / `serde_json` 本地配置；`tracing` 诊断。
- Cargo 依赖和 profile 见 [`Cargo.toml`](Cargo.toml)。

## 文档导航

| 文档 | 权威内容 |
|---|---|
| [`AGENTS.md`](AGENTS.md) | 修改约束、代码规范、门禁和交付流程 |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | 当前模块图、分层、状态权威和线程模型 |
| [`BACKEND.md`](BACKEND.md) | AudioEngine、WASAPI、解码和配置内部实现 |
| [`API.md`](API.md) | CLI 与进程内 Rust 调用契约（无 HTTP API） |
| [`FRONTEND.md`](FRONTEND.md) | Slint 页面、桥接状态、UI 契约和限制 |
| [`FILE-CONVENTIONS.md`](FILE-CONVENTIONS.md) | 目录职责、命名、规模、unsafe 与测试规范 |
| [`DEBUGGING.md`](DEBUGGING.md) | CLI、音频链路和 UI 快照排障 |
| [`goal.md`](goal.md) / [`docs/refactor-plan.md`](docs/refactor-plan.md) | 产品需求与 S0–S6 切片 |
| [`docs/step-*.md`](docs/) | 阶段实测记录；历史事实以最新记录为准 |

## 构建与冒烟

```powershell
.\scripts\check.cmd
.\scripts\smoke.cmd
.\scripts\build-dev.cmd test
.\dev.ps1
.\scripts\build-debug.cmd
.\scripts\build-release.cmd
```

`check.cmd` 包含格式、严格 Clippy、构建与行数门禁；`smoke.cmd` 枚举设备/进程，并生成播放器、路由、状态栏声纹Popover、窗口重开绘制、菜单、分类和三个真实音源窗口快照。
`dev.ps1` 默认启用 Slint Live Preview：`.slint` 文件由运行中的进程重载，Rust 改动仍会重编并重启；`scripts\dev.cmd -Release` 禁用开发热更新。
`build-debug.cmd` / `build-release.cmd` 分别产出 debug 与 release 的 exe 并打印路径。真实设备声音与物理鼠标仍需人工验收。

## 当前重构状态

S1–S6 重构阶段均有历史记录。最近变更：
- [`docs/step-76-window-reopen-fixes.md`](docs/step-76-window-reopen-fixes.md)：窗口首次定位改为显示前恢复/立即居中；独立窗口下一轮重绘；添加进程/音频到路由后隐藏子窗口且不启动全局转发。
- [`docs/step-75-native-movable-windows.md`](docs/step-75-native-movable-windows.md)：记录原生标题栏、位置持久化、独立窗口重绘与添加源交互基线。
- [`docs/step-69-route-source-details-and-sfx-peak.md`](docs/step-69-route-source-details-and-sfx-peak.md)：添加源窗口显示真实条目详情与SFX实时峰值，并补齐空分类区右键命中。
- [`docs/step-70-ui-routing-source-polish.md`](docs/step-70-ui-routing-source-polish.md)：修复音频库拖动/图标编辑、添加源三类入口、真实进程图标、实时声纹、路由源启动和顶层源菜单。
- [`docs/step-71-ui-regression-fixes.md`](docs/step-71-ui-regression-fixes.md)：统一添加源类型卡、缩短并对齐麦克风下拉，修复文件源启用误启动全局路由和分类图标原名保存，源菜单改为 PopupWindow。
- [`docs/step-72-multi-source-routing-performance.md`](docs/step-72-multi-source-routing-performance.md)：支持多进程/多音频加入、替换入口、删除源、文件不自动播放，并优化声纹轮询空闲CPU。
- [`docs/step-74-popup-dialog-windows.md`](docs/step-74-popup-dialog-windows.md)：将添加源/音频编辑拆成独立Slint Window；Step 75 改为可拖动原生窗口并收口位置/重绘。
- [`docs/step-73-microphone-forwarding-deadlock.md`](docs/step-73-microphone-forwarding-deadlock.md)：修复麦克风捕获线程停止标志未保存导致的永久 join 卡死。
- [`docs/step-67-audio-library-interactions.md`](docs/step-67-audio-library-interactions.md)：音频库可调三栏、分类过滤、分类编辑、条目编辑和默认音乐图标。
- [`docs/step-66-debug-build-script.md`](docs/step-66-debug-build-script.md)：修复双击 Debug 构建脚本时窗口闪退和工作目录依赖。
- [`docs/step-65-line-gate-policy.md`](docs/step-65-line-gate-policy.md)：将行数门禁改为常规目标、审查警告和硬上限三级策略。
- [`docs/step-64-audio-library-tab.md`](docs/step-64-audio-library-tab.md)：音频库 tab 红框区域、真实波形、详情操作和真实数据接线。
- [`docs/step-63-marquee-toggle-selection.md`](docs/step-63-marquee-toggle-selection.md)：增加跑马灯总开关，并让蓝色指示条只标记实际选中项。
- [`docs/step-62-marquee-dropdown.md`](docs/step-62-marquee-dropdown.md)：封装跑马灯文字/下拉列表并接入路由首页与进程选择。
- [`docs/step-61-live-preview.md`](docs/step-61-live-preview.md)：Slint 1.18 官方 Live Preview 与开发脚本，验证窗口进程内重载。
- [`docs/step-52-route-source-actions.md`](docs/step-52-route-source-actions.md)：源卡片状态保留、紧凑音量布局、源操作菜单与单独转发。
- [`docs/step-53-route-card-layout.md`](docs/step-53-route-card-layout.md)：路由源卡片统一布局、真实麦克风下拉、右侧菜单和底部音量控制组。
- [`docs/step-54-route-device-marquee.md`](docs/step-54-route-device-marquee.md)：输出设备长名称横向滚动与源菜单展开时隐藏 `...` 触发按钮。
- [`docs/step-55-route-icons-menu.md`](docs/step-55-route-icons-menu.md)：Miccat 路由主页删减、单源菜单、启用开关重排和 exe/文件图标显示。
- [`docs/step-56-miccat-branding-icons.md`](docs/step-56-miccat-branding-icons.md)：项目完整重命名为 Miccat，并修正路由源图标居中。
- [`docs/step-58-route-icon-centering.md`](docs/step-58-route-icon-centering.md)：修正路由卡片图标上下居中，并保留顶部并列按钮。
- [`docs/step-59-mic-icon-centering.md`](docs/step-59-mic-icon-centering.md)：按用户红框修正麦克风卡片图标相对整张卡片的上下居中。
- [`docs/step-60-source-icon-absolute-centering.md`](docs/step-60-source-icon-absolute-centering.md)：将源图标块移出横向布局并按整张卡片绝对居中。
- [`docs/step-50-home-layout-playback-hotkeys.md`](docs/step-50-home-layout-playback-hotkeys.md)：主页排版/卡片交互、播放暂停语义、右键菜单现代化、全局多热键与冲突检测。
- [`docs/step-49-user-feedback-fixes.md`](docs/step-49-user-feedback-fixes.md)：八项用户反馈修复（开发脚本、0x8889000A 根因、实时切换、输出下拉、列表点击）。
- [`docs/step-48-diagnostics.md`](docs/step-48-diagnostics.md)：全局错误诊断与调试输出系统（问题中心、运行时调试开关、日志落盘）。
- [`docs/step-47-route-controls.md`](docs/step-47-route-controls.md)：路由页交互接线审查。
- [`docs/step-46-library-tests.md`](docs/step-46-library-tests.md)：分离音频库单元测试，恢复 `library.rs` 行数门禁。
- [`docs/step-45-reference-ui-and-source-logic.md`](docs/step-45-reference-ui-and-source-logic.md)：参考图驱动的页面与音频行为。

本轮32个单元测试、`scripts\\smoke.cmd`、主/子窗口快照与添加源窗口重开像素一致性检查通过；`scripts\\build-dev.cmd fmt` 和 `scripts\\build-dev.cmd build` 通过。真实桌面鼠标位置恢复、分类/音频编辑窗口第二次物理显示和真实设备出声仍需人工验收。

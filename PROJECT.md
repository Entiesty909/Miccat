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
| [`goal.md`](goal.md) / [`docs/refactor-plan.md`](docs/refactor-plan.md) / [`docs/ui-bridge-audio-refactor-plan.md`](docs/ui-bridge-audio-refactor-plan.md) | 产品需求、S0–S6 历史计划与 UI Bridge/音频模块收口计划 |
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

- [`docs/step-55-header-and-pink-theme.md`](docs/step-55-header-and-pink-theme.md)：顶栏、窗口图标、淡粉主题、卡片可读性、图片 API 和渲染优化。
- [`docs/step-54-main-window-three-column-layout.md`](docs/step-54-main-window-three-column-layout.md)：三栏主窗口、状态栏排列/尺寸、音量浮层和刷新失效源清理。
- [`docs/step-53-route-audio-controls-and-dialog-layout.md`](docs/step-53-route-audio-controls-and-dialog-layout.md)：三类路由卡片、分类队列切歌、四种播放模式、文件槽进度及添加源窗口。
- [`docs/step-52-route-card-visual-polish.md`](docs/step-52-route-card-visual-polish.md)：路由卡片紧凑列、真实图标和滚动条避让。
- [`docs/step-51-popup-dialog-windows.md`](docs/step-51-popup-dialog-windows.md)：独立窗口呈现、原生拖动、位置恢复和重复显示重绘。
- [`docs/step-50-multi-source-routing.md`](docs/step-50-multi-source-routing.md)：多源、卡片状态隔离、重复源校验、麦克风关停与空闲性能。
- [`docs/step-49-route-source-details-and-sfx-peak.md`](docs/step-49-route-source-details-and-sfx-peak.md)：真实音源详情/峰值、交互接线和 UI 回归。
- [`docs/step-48-audio-library-interactions.md`](docs/step-48-audio-library-interactions.md)：音频库分类/条目管理、播放设置归属和路由文件手动播放。
- [`docs/step-47-marquee-dropdown.md`](docs/step-47-marquee-dropdown.md)：音频库三栏页面、长文本跑马灯及选中/计时开关。
- [`docs/step-46-live-preview.md`](docs/step-46-live-preview.md)：Live Preview、Debug 构建提示和行数门禁。
- [`docs/step-45-miccat-branding-icons.md`](docs/step-45-miccat-branding-icons.md)：应用统一重命名为 Miccat。
- [`docs/step-44-home-layout-playback-hotkeys.md`](docs/step-44-home-layout-playback-hotkeys.md)：主页交互、播放器、菜单、全局热键和路由卡片/添加源布局。
- [`docs/step-43-user-feedback-fixes.md`](docs/step-43-user-feedback-fixes.md)：八项用户反馈修复。
- [`docs/step-42-diagnostics.md`](docs/step-42-diagnostics.md)：全局问题中心、运行时调试开关和日志。
- [`docs/step-41-ui-redesign.md`](docs/step-41-ui-redesign.md)：三页视觉重排与音频交互接线。

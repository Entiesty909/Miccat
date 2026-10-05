# PROJECT.md — ProcessMic

## 项目定位

ProcessMic 是 Windows 桌面进程音频路由器：以 WASAPI Process Loopback 捕获指定进程树，
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

```bat
scripts\check.cmd
scripts\smoke.cmd
scripts\build-dev.cmd test
scripts\dev.cmd
scripts\build-debug.cmd
scripts\build-release.cmd
```

`check.cmd` 包含格式、严格 Clippy、构建与行数门禁；`smoke.cmd` 枚举设备/进程，生成默认 UI 和条目菜单快照，并模拟空树右键且断言菜单打开。
`dev.cmd` 监听 `src`/`ui`/`Cargo.toml` 改动，自动重编并重启（`-Release`、`-NoRun` 可选）；
`build-debug.cmd` / `build-release.cmd` 分别产出 debug 与 release 的 exe 并打印路径。
真实设备声音与物理鼠标仍需人工验收。

## 当前重构状态

S1–S6 重构阶段均有历史记录。最近变更：
- [`docs/step-52-route-source-actions.md`](docs/step-52-route-source-actions.md)：源卡片状态保留、紧凑音量布局、源操作菜单与单独转发。
- [`docs/step-50-home-layout-playback-hotkeys.md`](docs/step-50-home-layout-playback-hotkeys.md)：主页排版/卡片交互、播放暂停语义、右键菜单现代化、全局多热键与冲突检测。
- [`docs/step-49-user-feedback-fixes.md`](docs/step-49-user-feedback-fixes.md)：八项用户反馈修复（开发脚本、0x8889000A 根因、实时切换、输出下拉、列表点击）。
- [`docs/step-48-diagnostics.md`](docs/step-48-diagnostics.md)：全局错误诊断与调试输出系统（问题中心、运行时调试开关、日志落盘）。
- [`docs/step-47-route-controls.md`](docs/step-47-route-controls.md)：路由页交互接线审查。
- [`docs/step-46-library-tests.md`](docs/step-46-library-tests.md)：分离音频库单元测试，恢复 `library.rs` 行数门禁。
- [`docs/step-45-reference-ui-and-source-logic.md`](docs/step-45-reference-ui-and-source-logic.md)：参考图驱动的页面与音频行为。

本轮源逻辑构建、Clippy、测试和路由快照结果以 Step 52 实测为准。

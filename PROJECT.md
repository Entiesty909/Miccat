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
```

`check.cmd` 包含格式、严格 Clippy、构建与行数门禁；`smoke.cmd` 枚举设备/进程并生成 UI 默认态和条目菜单快照。真实设备声音与 Windows 桌面交互仍需人工验收。

## 当前重构状态

S1–S6 均有中文阶段提交。`scripts/check.cmd` 的严格 Clippy、构建和行数门禁通过；
2 个音频库领域单测和设备/进程/UI 快照冒烟通过。

重构计划仍有未完成验收：RAM/CPU/启动时间基线、真实 `--capture` 逐采样对比、`--route` 听感和桌面交互；
S3 滚动/虚拟化、S4 capture/engine 与 PCM 原语拆分、S5 config/UI 文案映射仍未完成。
S6 的剩余可见性收口及 PCM/WAV/配置迁移测试也待补。
`src/engine.rs`、`src/audio/capture.rs`、`ui/app-window.slint` 仍高于常规行数预算，门禁只限制不增长。

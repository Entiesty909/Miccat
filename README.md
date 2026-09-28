# ProcessMic

轻量 Windows 进程音频路由器。把指定进程的声音经 WASAPI Process Loopback 送到所选输出设备。

原 C# DRG SoundPad 仅作功能参考，见 `docs/step-01-architecture-analysis.md`。Rust 实现不是逐行移植。

## 要求

- Windows 10 21H1+ / Windows 11（Process Loopback）
- 本机已用 `stable-x86_64-pc-windows-msvc` 构建

## 构建

```bat
call "D:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
cargo build --release
```

或 `scripts\build-dev.cmd build --release`

## 运行

```text
target\release\mintcat-proc-mic.exe                 UI
--list-processes
--list-devices
--capture <pid> --wav capture.wav --seconds 5
--route <pid> --device <wasapi-id> --seconds 8
```

日志：`RUST_LOG=info`。音频热路径不打日志。

## 文档

| 文件 | 内容 |
|---|---|
| `PROJECT.md` | 产品边界、技术栈与文档导航 |
| `ARCHITECTURE.md` | 当前模块、依赖方向、状态权威与线程模型 |
| `BACKEND.md` | AudioEngine、安全 API、音频链路与持久化 |
| `API.md` | CLI 与进程内 Rust 调用契约（无 HTTP API） |
| `FRONTEND.md` | Slint 页面、桥接状态与 UI 契约 |
| `FILE-CONVENTIONS.md` | 目录职责、命名、规模、unsafe 与测试规范 |
| `DEBUGGING.md` | CLI、音频链路与 UI 快照定位路径 |
| `goal.md` | 产品需求、禁止项与验收标准 |
| `docs/refactor-plan.md` | S0–S6 切片和剩余验收 |
| `docs/step-02-rust-architecture.md` | Rust 目录、线程和音频模型 |
| `AGENTS.md` | 智能体约束与交付流程 |
| `docs/step-*.md` | 分阶段实测记录与历史架构分析 |

## 许可

MIT

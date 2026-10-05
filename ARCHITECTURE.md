# ARCHITECTURE.md — ProcessMic 当前架构

## 产品与技术边界

ProcessMic 是 Windows 本地桌面程序：以 WASAPI Process Loopback 捕获指定进程，混音后送入 render 设备。
本地音频库支持 SFX 路由和本机试听。

技术栈：Rust 2024 单 crate、Slint、windows-rs 0.62、WASAPI、Media Foundation、Serde JSON 和 tracing。
没有 Web 服务、HTTP API、数据库、网络账户或异步运行时。

## 目录职责

| 路径 | 职责 |
|---|---|
| `src/main.rs` | DPI、日志初始化、CLI/UI 组合根 |
| `src/cli/mod.rs` | argv 派发、设备/进程命令、capture/route 与播放探针 |
| `src/ui_bridge/` | Slint 组装、AppState、回调、缓存、持久化、状态轮询和异步设备/进程列表刷新 |
| `src/engine.rs` | AudioEngine 权威状态、进程/麦生命周期、多输出打开与停止 |
| `src/engine/{fanout,monitor,playback,volume,status,peaks}.rs` | 多输出转换、本机试听、文件播放、增益、状态与峰值 API |
| `src/audio/` | WASAPI capture/render、ring、格式转换、MF/WAV 解码、SFX、试听、CLI dump |
| `src/library.rs`、`src/library/tests.rs` | 音频库领域规则与私有逻辑单元测试 |
| `src/config/` | AppConfig 模型、JSON 存储/迁移、Windows 单实例互斥 |
| `src/platform/` | 跨层共享的 COM 初始化 |
| `src/process.rs`、`src/hotkey.rs`、`src/vbcable.rs` | 进程枚举、RegisterHotKey、VB-CABLE 安装 |
| `src/diagnostics/` | 运行时调试开关、tracing 捕获、日志落盘与全局问题中心 |
| `src/platform/clipboard.rs` | Win32 剪贴板文本写入（问题中心复制详情） |
| `ui/app-window.slint` | 主窗口、顶栏、状态栏和全局右键菜单覆盖层 |
| `ui/overlays/global-overlays.slint` | `DiagState` 全局状态与问题中心覆盖层宿主 |
| `ui/components/page-host.slint` | 装配路由、音频库、设置三页并转发页面回调 |
| `ui/pages/`、`ui/components/` | 页面内容与无业务权威的可复用控件；`add-source-dialog.slint` 和 `source-actions-menu.slint` 提供路由源操作 UI |

完整文件树和文件规模规则见 [`FILE-CONVENTIONS.md`](FILE-CONVENTIONS.md)。

## 依赖方向

```text
main.rs
├─ cli/ ───────────────→ process / AudioEngine / audio
└─ ui_bridge/
   ├─ engine.rs + engine/ ─────────→ audio/
   ├─ library.rs ──────────────────→ config::AudioEntry / audio::decode（文件验证）
   ├─ config/ / process / hotkey
   └─ audio::device/policy / dialogs / vbcable（当前基础设施直连）

process ───────────────→ platform::com
audio/com ─────────────→ platform::com
```

- `engine` 和 `audio` 不依赖 Slint；`process` 不依赖 `audio`。
- `engine/playback.rs` 当前以 `config::AudioEntry` 接收库条目，是尚待收口的领域依赖。
- `audio::com` 是对 `platform::com` 的内部转发。
- 桥接层不直接调用 `audio::decode`、`audio::sfx`、`audio::preview`；播放/试听走 AudioEngine。
- `ui_bridge/lists.rs` 仍枚举 `audio::device`，路由回调仍调用 `audio::policy`；原生对话框和 VB-CABLE 安装也未迁入 platform。

## 状态权威

- `AudioEngine`：进程/麦/输出运行态、音量、电平、试听和文件播放控制。
- `AppConfig`：本地持久化配置，路径 `%APPDATA%\ProcessMic\config.json`。
- `AppState`：桥接层共享句柄，不是第三份音频运行时。
- `UiCache` 保存筛选、索引、菜单/点击态和列表快照；其中仍复制部分配置数据，
  AppConfig/UiCache 单一权威尚未完全收敛。
- `src/ui_bridge/sources.rs`：将进程、当前音频文件、物理麦克风的实时状态投影为路由源行；不持有第二份引擎状态。
- `OutputConfig.device_ids`：持久化多选 render 设备 ID；UI 使用 friendly name 显示，按 ID 恢复。
Slint 属性只作为显示投影和事件契约，不保存 PCM。

## 运行线程

```text
UI / 50 ms 状态轮询
├─ route commands → AudioEngine
│  ├─ Process Loopback capture threads → source SPSC rings
│  ├─ optional physical-mic capture → source SPSC ring
│  ├─ SFX mixer thread → SFX source ring
│  ├─ fanout thread → per-target rings
│  └─ one WASAPI render thread per output
├─ audio library → engine playback coordinator → bounded decoder streams
│  ├─ SFX mixer (when route outputs are active)
│  └─ monitor source/render (when local preview is enabled)
└─ config change → 300 ms single-shot debounce → serial config writer
```

Capture、fanout、render 热循环使用预分配缓冲和固定容量 SPSC ring；不得分配扩容、写文件、
访问 UI 或执行阻塞锁。解码通过有界同步通道分块交付。
配置写线程按 FIFO 保存快照，退出时 flush 后 join。

## 音频 API 与错误

- Process Loopback 通过 `ActivateAudioInterfaceAsync` 并包含目标进程树；不以 endpoint loopback 猜测进程声。
- `AudioEngine::open_outputs()` 建立每目标 render；`set_process_source()`、`add_process_source()`
  和 `set_mic_source()` 控制输入。
- `play_file(entry, start_hns, target)`、`stop_file()`、`file_state()` 管理音频库流；`PlaybackTarget` 选择路由、本机监听或双路。
- 错误使用 `Error` 与 `EngineStatus`；UI 展示 `user_message()`，底层 HRESULT/细节走 tracing。
- `src/diagnostics/` 是全局诊断权威：`HubLayer` 捕获 WARN/ERROR 到问题中心（`ui_bridge` target 除外，
  由桥接层用 `report_error` 上报用户短句与建议）；调试开关按组件分类控制日志文件级别，
  控制台固定 info，问题中心不受开关影响。
- 本机麦克风是捕获输入，不是 WASAPI 播放终点；虚拟线缆使用其播放端写入、录音端供语音软件选择。
- **同一虚拟驱动的两个端点不能同时打开**：实测 VB-Cable 的「CABLE虚拟扬声器」与
  「CABLE In 16ch」第二个 `Initialize` 返回 `AUDCLNT_E_DEVICE_IN_USE`（`0x8889000A`）。
  判定按驱动接口名（`PKEY_DeviceInterface_FriendlyName`）分组；路由启动时把冲突目标
  自动折叠到同一设备，引擎层保留可读错误兜底。
- **试听（monitor）会占用系统默认播放设备**：`AudioEngine::stop()` 必须释放它，
  否则随后对同一设备（或同驱动另一端点）的转发会撞 `0x8889000A`。

## 已知待收口事实

- `UiCache` 仍镜像一部分 AppConfig 字段。
- `Hotkey::label()`、`slint_key_to_vk()` 仍在 config 模块；`process.rs::label()` 仍产出 UI 文案。
- 文件导入回调仍同步验证媒体文件；路由停止调用可能等待线程 join。
- `engine.rs`、`audio/capture.rs`、`ui/app-window.slint` 仍超过常规文件行数预算；行数门禁只允许它们不高于当前基线。
- 真实设备路由、capture 逐采样对比、桌面交互和 RAM/CPU/启动基线尚未实测；
  阶段证据见 [`docs/step-*.md`](docs/) 和 [`DEBUGGING.md`](DEBUGGING.md)。

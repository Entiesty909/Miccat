# ARCHITECTURE.md — Miccat 当前架构

## 产品与技术边界

Miccat 是 Windows 本地桌面程序：以 WASAPI Process Loopback 捕获指定进程，混音后送入 render 设备。
本地音频库支持 SFX 路由和本机试听。

技术栈：Rust 2024 单 crate、Slint 1.18、windows-rs 0.62、WASAPI、Media Foundation、Serde JSON 和 tracing。
没有 Web 服务、HTTP API、数据库、网络账户或异步运行时。
`slint/live-preview` 仅由 `dev.ps1` 在开发构建时通过命令行启用；发布构建不包含该 feature。

## 目录职责

| 路径 | 职责 |
|---|---|
| `src/main.rs` | DPI、日志初始化、CLI/UI 组合根 |
| `src/cli/mod.rs` | argv 派发、设备/进程命令、capture/route 与播放探针 |
| `src/ui_bridge/` | Slint 组装、AppState、回调、缓存、持久化、状态轮询、异步设备/进程刷新和独立窗口生命周期 |
| `src/ui_bridge/window_lifecycle.rs` | 独立窗口回调、显示/隐藏、物理位置保存、添加源窗口属性同步 |
| `src/engine.rs` | AudioEngine 权威状态、进程/麦/输出生命周期、多输出打开与停止 |
| `src/engine/{fanout,monitor,playback,volume,status,peaks}.rs` | 多输出转换、本机试听、文件播放、增益、状态与峰值 API |
| `src/audio/` | WASAPI capture/render、COM façade、SPSC ring、格式转换、MF/WAV 解码、SFX、试听、CLI WAV 导出 |
| `src/audio/errors.rs` | 统一分类 capture/render 设备失效 HRESULT |
| `src/library.rs`、`src/library/tests.rs` | 音频库分类/索引/递归变更规则、分类图标路径迁移及私有逻辑单测 |
| `src/config/` | AppConfig 模型、JSON 存储/迁移、窗口物理坐标和 Windows 单实例互斥 |
| `src/presentation.rs` | CLI 与桥接共用的进程、热键展示格式；不持有配置或运行态 |
| `src/platform/` | 跨层共享的 COM 初始化与独立 Window 前置/居中辅助 |
| `src/process.rs`、`src/hotkey.rs`、`src/vbcable.rs`、`src/platform/icon.rs` | 进程枚举、RegisterHotKey、VB-CABLE 安装、Win32 exe/文件图标提取 |
| `src/diagnostics/` | 运行时调试开关、tracing 捕获、日志落盘与全局问题中心 |
| `src/platform/clipboard.rs` | Win32 剪贴板文本写入（问题中心复制详情） |
| `ui/app-window.slint` | 主窗口、顶栏、状态栏、实时源声纹 Popover 和全局右键菜单覆盖层 |
| `ui/overlays/diagnostic-panel.slint`、`ui/overlays/global-overlays.slint` | 右侧抽屉式问题中心（问题列表 + 日志开关）、`DiagState` 全局状态与顶层覆盖层宿主；状态栏与设置页直接读写该全局，不经 `app-window` 透传。 |
| `ui/components/diagnostic-row.slint`、`ui/components/diagnostics-card.slint` | 问题行/徽标与设置页诊断卡片。 |
| `ui/components/page-host.slint` | 装配路由、音频库、设置页、音频库三栏拖动宽度和独立编辑窗口事件转发；音频库详情只展示管理信息，路由页拥有输出/增益等播放设置。 |

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
- `AppConfig`：本地持久化配置，路径 `%APPDATA%\\Miccat\\config.json`。
  - `AppConfig.audio_category_icons`：分类路径到 glyph/图标文件路径的持久化映射。
  - `AudioEntry.name`、`icon_glyph`、`icon_path`：软件显示名称与图标配置；不改磁盘文件名。
  - `src/ui_bridge/sources.rs`：将进程、当前音频文件和物理麦克风投影为路由源行；麦克风源行固定显示“麦克风”，副标题使用所选物理设备的 Windows 友好名称，`update_dynamic` 只写实时峰值/音量/状态，不重建静态模型。
 - `UiCache` 的 `process_volumes` / `audio_volumes` 与对应输出 ID 按卡片槽位保存；卡片操作不再读写同一 kind 的共享增益或输出目标。
- `AudioEngine` 为最多 4 个进程槽和 4 个文件槽保留独立原子增益；SFX 槽位状态区分停止、播放、暂停和完成，暂停保持解码位置。
- `UiCache.route_audio_entry`：待加入路由或当前路由使用的音频条目实际下标；仍是桥接派生状态，不替代引擎播放状态。
- `UiCache.route_file_enabled`：用户为文件源选择的启用状态；与全局路由启停和文件播放器播放状态分离。
- `UiCache.route_audio_entries` 与 `route_audio_enabled`：多个已加入路由的音频条目及其启用状态；不保存 PCM。
  运行期播放状态由引擎 SFX mixer 的槽位原子状态提供，UI 每 50 ms 更新对应行。
- `OutputConfig.device_ids` 持久化多选 render 设备 ID；`process_output_ids` / `file_output_ids` 保存卡片级目标。
- `OutputConfig.mic_output_id` 保存麦克风源可选目标；未设置时跟随路由主输出。
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

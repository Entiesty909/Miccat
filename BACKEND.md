# BACKEND.md — 本地引擎与基础设施

## “后端”的含义

Miccat 没有服务端进程、HTTP/REST API、WebSocket、登录、数据库或网络协议。
本文“后端”指本地 Rust 引擎、音频库领域规则、配置存储和 Windows 适配层。
系统接口包括 WASAPI、Media Foundation、Process API 和 Win32/COM。

## 模块职责

- `src/engine.rs`：`AudioEngine` 权威持有运行态，管理输出、进程源、物理麦、音量、电平与生命周期。
- `src/engine/fanout.rs`：将进程/麦/SFX 源 ring 按每个 render 目标格式转换并推送。
- `src/engine/monitor.rs`：引擎拥有的本机试听输出；试听不是虚拟麦克风输入。
- `src/engine.rs`：`AudioEngine` 权威持有运行态，管理输出、进程源、物理麦、音量、电平与生命周期；capture 源提供窗口峰值读取。
- `src/audio/`：WASAPI capture/render、COM façade、SPSC ring、格式转换、MF/WAV 解码、SFX、试听和 CLI WAV 导出。
- `src/library.rs`：分类/索引/递归变更规则、分类图标路径迁移及音频验证。
- `src/config/`：配置数据、JSON 读写/旧格式迁移、单实例互斥；`AudioEntry` 保存媒体与软件显示元数据，`AppConfig.windows` 保存主窗口及两个独立窗口的物理坐标。
- `src/platform/`：跨层共享的 COM 初始化与独立Window前置、工作区居中辅助。
- `src/process.rs` / `src/hotkey.rs` / `src/vbcable.rs`：进程枚举、RegisterHotKey、VB-CABLE 安装。

## AudioEngine 内部调用接口

```rust
AudioEngine::new()
AudioEngine::open_output(device_id)
AudioEngine::open_outputs(device_ids)
AudioEngine::set_process_source(pid)
AudioEngine::add_process_source(pid, slot)
AudioEngine::set_mic_source(mic_id)
AudioEngine::set_volume(value)
AudioEngine::set_mic_volume(value)
AudioEngine::set_sfx_volume(value)
AudioEngine::set_master_volume(value)
AudioEngine::volume() / mic_volume() / sfx_volume()
AudioEngine::set_monitor_default(enabled)
AudioEngine::set_monitor_volume(value)
AudioEngine::play_file(entry, start_hns, target)
AudioEngine::stop_file()
AudioEngine::file_state()
AudioEngine::stop()
```

`PlaybackTarget::{Both, Monitor, Route}` 分别选择路由与本机监听双路、本机 monitor 或活动路由 SFX mixer。
`play_file()` 在引擎后台解码；`file_state()` 返回状态、名称、路径、时长、位置和错误。
当前 seek 由 UI 停止旧流后以目标 `start_hns` 重新调用 `play_file()`，没有独立 `seek_file()` API。

`OutputConfig.device_ids` 保存用户多选的 render 设备 ID；刷新设备列表按 ID 恢复选择。
源卡片进程开关只影响 `UiCache.disabled_pids`，卡片保留；删除进程源才从 `selected_pids` 移除。
文件源开关只更新 `UiCache.route_file_enabled` 或对应 `route_audio_enabled` 槽位；路由未运行时不会启动全局路由，只有状态栏“开始转发”负责启动。运行中的启用文件通过 `play_file_source()` 追加到 SFX mixer，不停止其它文件流。
`UiCache.route_audio_entries` 保存多个待加入路由的真实音频条目；无进程时，已启用文件源或已选麦克风也可以启动完整路由。
设备 ID 列表为空时，启动路由回退为单个 `device_id`。
源行中的进程/文件/麦克风音量调用相应 AudioEngine 原子增益并写入 `MixConfig`。
路由链路：

```text
PID → Process Loopback capture → 源 SPSC ring
    → fan-out 格式转换 → 每目标 SPSC ring → WASAPI render
可选麦 capture ────────────────────────────┘
音频库解码 → SFX mixer ────────────────────┘
```

- 每个输出目标有独立 render 线程。Capture、fan-out、SFX 和 render 循环不得扩容、写文件、访问 UI、打日志或等待普通阻塞锁。进程、麦克风与 SFX 峰值采样只写 `AtomicU32`；UI 线程按 50 ms 轮询读取并清零。
- 源行 50 ms 更新只写变化的电平/状态模型单元；静态图标和下拉模型不在轮询中重建。

## 本机试听与文件播放

- `set_monitor_default(true)` 在后台打开系统默认 render 设备；`false` 关闭它。
- `play_file()` 按 `PlaybackTarget::{Both, Monitor, Route}` 分别监听+路由、仅监听、仅路由；
  `set_monitor_volume()` 只调整本机 monitor，不改变路由 SFX 增益。
- 路由文件源增益仍走 `set_sfx_volume()`，与本机监听音量是独立控制。
- 音频库分类、索引、递归变更和分类图标路径迁移由 `src/library.rs` 处理；条目名称与图标由 `AppConfig` 持久化。
- 音频条目编辑只修改 `AudioEntry.name`、`icon_glyph`、`icon_path`；不重命名磁盘文件。

## 配置与持久化

配置路径为 `%APPDATA%\\Miccat\\config.json`。
`config::load()` / `config::save()` 从 `config/mod.rs` re-export；实现位于 `config/store.rs`。
读配置处理缺省、损坏和旧热键格式；写入失败只记录 tracing。

UI 修改经 `ui_bridge/persist.rs::SaveQueue` 防抖 300 ms，再由 FIFO 单写线程落盘。
退出时写入最终快照并 join；滑块回调不逐事件写盘。

## 组合根与 CLI

`src/main.rs` 只负责 DPI、tracing 初始化和组合 CLI/UI 入口。命令派发位于 `src/cli/mod.rs`：

```text
无参数                启动 UI
--list-processes      枚举进程
--list-devices        枚举播放/录音设备
--capture <pid>       Process Loopback 写 WAV
--route <pid>         临时路由到 --device <wasapi-id>
--snapshot-ui         软件渲染 UI RGBA 快照
--probe-playback      文件流播放逐采样探针
```

`--capture`、`--route`、`--probe-playback` 是本地诊断入口，不构成网络 API。

## 当前边界与待收口点

- `ui_bridge/` 不直接调用 `audio::decode`、`audio::sfx`、`audio::preview`；播放走 AudioEngine。
- `ui_bridge/lists.rs` 和路由回调仍直接访问 `audio::device` / `audio::policy`。
- `process.rs::label()`、`config::Hotkey::label()`、`config::slint_key_to_vk()` 仍含 UI 文案/映射职责。
- 真实设备 route/capture 听感必须实机验收；`cargo test` 不能替代。

## UI 线程响应性注意

- `AudioEngine::stop()` 会 join capture、SFX、fanout 和 render 线程；不得从长任务路径阻塞调用。
- 路由停止回调当前同步调用 `AudioEngine::stop()`，这是已知 UI 响应性风险。
- 导入回调当前同步执行 WAV/MF 文件探测，应移到工作线程。
- 引擎线程 join、设备操作和文件 IO 不属于实时循环；Slint 回调仍须遵守 `AGENTS.md` 限制。

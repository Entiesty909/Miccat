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
AudioEngine::open_routed_outputs(device_ids, SourceOutputRoutes)
AudioEngine::set_process_source(pid)
AudioEngine::add_process_source(pid, slot)
AudioEngine::set_process_volume(slot, value)
AudioEngine::set_sfx_volume_for_slot(slot, value)
AudioEngine::set_mic_source(mic_id)
AudioEngine::set_volume(value) / set_mic_volume(value) / set_sfx_volume(value)
AudioEngine::set_master_volume(value)
AudioEngine::volume() / process_volume(slot) / mic_volume() / sfx_volume_for_slot(slot)
AudioEngine::set_mic_preview_volume(value) / set_mic_preview_source(mic_id)
AudioEngine::play_file(entry, start_hns, target)
AudioEngine::play_file_source(entry, slot, looped, start_hns, paused)
AudioEngine::stop_file_source(slot)
AudioEngine::set_file_source_paused(slot, paused)
AudioEngine::file_source_state(slot) / file_source_position_hns(slot)
AudioEngine::stop_file()
AudioEngine::file_state()
AudioEngine::stop()
```

`SourceOutputRoutes` 为进程槽和音频文件槽分别保存 render 下标；未使用独立路由时，`open_outputs()` 将所有槽位送到全部目标。进程/文件增益按槽位使用原子值，渲染线程不会因修改一张卡片而读取另一张卡片的增益。

`play_file_source(entry, slot, looped, start_hns, paused)` 从指定的 100 ns 媒体位置解码，在 SFX mixer 中替换同槽旧流、保留其它槽流；`stop_file_source(slot)` 只停止指定文件卡片。每槽的命令代数锁防止较早的解码线程在拖动进度或停止后覆盖较新的流；锁只用于控制线程，不进音频实时循环。暂停/恢复通过槽位命令保持当前解码位置；状态原子提供停止、播放、暂停和自然完成，位置原子以 100 ns 为单位供 UI 轮询。
路由文件卡片播放始终先使用 `play_file(..., PlaybackTarget::Monitor)` 走本机试听；未启用转发和未运行路由不创建 SFX 槽流。运行中已启用的文件卡片另从同一媒体位置调用 `play_file_source()`；关闭转发仅停止该路由槽，继续本机播放，重新启用时若仍在播放才从当前进度接入。分类队列、前后切歌、四种播放模式和自然结束轮询不再以路由开关为先决条件。全局/单独路由重建输出图前记录正在试听的文件位置，之后恢复本机试听，且只在对应文件源已启用时恢复路由输出。单例试听槽与独立文件 SFX 槽分工明确；库内播放器启动其它文件会替换单例试听而不误启用卡片。
路由文件卡片只读取对应槽位的 `file_output_ids`；播放页的 `file_default_output_id` 仅供新卡片选取初始目标，不会覆盖现有文件卡片。旧 `file_output_id` 在读盘时迁移，保存时不写旧字段。配置的多选 ID 按可见端点记录；扬声器和配对虚拟麦克风虽写同一 render，录音端 ID 仍独立保存，仅在启动前解析为 render ID。
启动时 `route_start::routed_outputs` 仅收集已启用源的 render ID；禁用文件槽仍保留有效占位索引，不打开其原设备。同一 render 的扬声器与配对虚拟麦克风选择在运行时折叠为单输出；同驱动但不同 render 端点同时打开仍由引擎拒绝并保留原错误。

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
路由卡片本机文件试听从 `MixConfig.file_preview_volumes[slot]` 读取独立增益，旧槽回退到 `AppConfig.preview_volume`；播放页顶栏滑块编辑全局试听值，转发 SFX 槽仍只看 `mix.sfx_volumes`。麦克风监听采用独立增益与本机播放端，`MixConfig.mic_preview_volume` 默认为零；不会修改 `mix.mic` 或转发麦克风掩码。麦克风监听空闲时可独立于转发运行，启停链路不得阻塞 Slint 回调。
- 音频库分类、索引、递归变更和分类图标路径迁移由 `src/library.rs` 处理；条目名称与图标由 `AppConfig` 持久化。
- 音频条目编辑只修改 `AudioEntry.name`、`icon_glyph`、`icon_path`；不重命名磁盘文件。

## 配置与持久化

配置路径为 `%APPDATA%\\Miccat\\config.json`。
`config::load()` / `config::save()` 从 `config/mod.rs` re-export；实现位于 `config/store.rs`。
读配置处理缺省、损坏和旧热键格式；`AppConfig.theme_index` 持久化主题索引（0 浅色、1 深色、2 淡粉），旧配置缺字段时默认浅色。
写入失败只记录 tracing。

UI 修改经 `ui_bridge/persist.rs::SaveQueue` 防抖 300 ms，再由 FIFO 单写线程落盘。
主题选择写回同一个配置对象；退出时写入最终快照并 join；滑块回调不逐事件写盘。

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
- `ui_bridge/lists.rs` 和路由回调仍直接访问 `audio::device` / `audio::policy`，这是后续基础设施收口点。
- `src/presentation.rs` 承担 CLI 与桥接共用的进程/热键展示格式；`config/` 与 `process.rs` 只保存数据和枚举逻辑。
- `src/audio/errors.rs` 统一 capture/render 设备失效 HRESULT 的领域映射。
- `UiCache` 已移除设备筛选和默认设备配置镜像；设备配置从 Slint 投影或 AppConfig 读取，卡片级派生态仍由桥接缓存持有。
- 真实设备 route/capture 听感必须实机验收；`cargo test` 不能替代。

## UI 线程响应性注意

- `AudioEngine::stop()` 会 join capture、SFX、fanout 和 render 线程；不得从长任务路径阻塞调用。
- 路由停止回调当前同步调用 `AudioEngine::stop()`，这是已知 UI 响应性风险。
- 导入回调当前同步执行 WAV/MF 文件探测，应移到工作线程。
- 引擎线程 join、设备操作和文件 IO 不属于实时循环；Slint 回调仍须遵守 `AGENTS.md` 限制。
- 淡粉背景下载、解码、裁剪与预合成均在后台线程；无效 HTTP/图片响应最多额外重试三次，UI 线程只安装最终不透明位图。

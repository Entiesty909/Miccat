# Step 2 — Rust 架构

基于 `docs/step-01-architecture-analysis.md`。这不是 C# 类图的翻译。

产品：Windows 专用、极轻的 **Process Audio Router / Soundpad**。
栈：Rust 2024 + Slint + windows-rs + WASAPI。
当前路线：S1 UI 骨架已完成；S2 输出常驻与动态源已完成；S3+ 继续接配置、音频库、文件播放器与多模式热键。

---

## 1. 一句话

用户选择一个或多个进程、可选物理麦和一个或多个 render 设备；进程声必须使用 WASAPI Process Loopback。

```text
process PIDs → capture threads ─┐
optional physical mic ──────────┼→ source SPSC rings → fanout → per-target rings → render
library decode → SFX mixer ────┘                         ↘ monitor render（可选）
```

---

## 2. 目录

C# 源码已移除，分析结论见 `docs/step-01-architecture-analysis.md`，原始文件见 git 历史。Rust 工程在仓库根目录，不另起 monorepo。

```text
Cargo.toml
build.rs
ui/
  app-window.slint
  theme.slint
  overlays/context-menu.slint
  components/
  pages/
src/
  main.rs
  cli/mod.rs
  config/mod.rs
  config/store.rs
  config/single_instance.rs
  engine.rs
  engine/fanout.rs
  engine/monitor.rs
  engine/playback.rs
  error.rs
  hotkey.rs
  library.rs
  library/tests.rs
  platform/mod.rs
  platform/com.rs
  process.rs
  vbcable.rs
  audio/
    mod.rs
    buffer.rs
    capture.rs
    com.rs              platform COM 转发
    decode.rs
    device.rs
    dump.rs
    format.rs
    peak.rs
    policy.rs
    preview.rs
    render.rs
    sfx.rs
    wav.rs
  ui_bridge/
    mod.rs
    state.rs
    cache.rs
    lists.rs
    library.rs
    format.rs
    persist.rs
    status.rs
    input.rs
    window.rs
    dialogs.rs
    callbacks/
      mod.rs
      route.rs
      route_start.rs
      route_start/source_only.rs
      library.rs
      library_entries.rs
      library_metadata.rs
      player.rs
      settings.rs
    dev/snapshot.rs
docs/
  step-*.md
```

## 3. 线程

```text
Slint UI / 50 ms 状态轮询
    ├─ 路由命令 ─────────────→ AudioEngine
    │   ├─ Process Loopback × 4 ─┐
    │   ├─ optional physical mic ─┼→ source SPSC rings
    │   └─ SFX mixer ─────────────┘
    │          ├─ fanout → per-target rings → WASAPI render per output
    │          ├─ file playback → bounded decode streams → SFX/monitor
    │          └─ optional monitor source → local WASAPI render
    └─ config changes → 300 ms debounce → serial writer; exit flush/join
```

`open_outputs()` 为最多 4 个进程槽、麦克风和 SFX 建源 ring；每个输出目标有独立目标 ring 与 render 线程。
Process/Mic capture、render 和 monitor render 在线程内初始化 COM MTA；fanout 和 SFX mixer 不调用 UI。
文件解码经 32 项有界同步通道分块交付，播放状态由引擎持有。

每条音频 ring 都保持 SPSC；生产者/消费者固定，容量和转换暂存于启动时分配。
实时循环不得扩容、写文件、访问 UI、打日志或等待普通阻塞锁。
关停通过原子标志通知并 join；试听源停止请求将 join 移出 Slint 回调。

---

## 4. 音频细节

### 4.1 Capture

Windows 10 20348+：

- `ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, IAudioClient, PROPVARIANT(VT_BLOB of AUDIOCLIENT_ACTIVATION_PARAMS), handler)`
- `ActivationType = PROCESS_LOOPBACK`
- `TargetProcessId = pid`
- `ProcessLoopbackMode = INCLUDE_TARGET_PROCESS_TREE`
- `IAudioClient::Initialize(SHARED, LOOPBACK | EVENTCALLBACK, 0, 0, mixFormat, NULL)`
- `IAudioCaptureClient` + event handle

`unsafe` 只在 WASAPI、COM/MF、Win32 等互操作模块内使用；每处按 `AGENTS.md` 写 SAFETY 条件。

### 4.2 Render

- `IMMDeviceEnumerator::GetDevice(id)` → `Activate IAudioClient`
- mix format、SHARED + EVENTCALLBACK
- `IAudioRenderClient`
- ring 空：写静音；ring 满：capture 丢最旧，保活延迟

### 4.3 内部格式

不硬编码 48k / stereo / f32。  
ring 存 **render mix 的 f32 交错帧**。capture 线程做：

- PCM/float/extensible → f32
- 声道 upmix/downmix（mono↔N，>2 降到 render 声道，多出来的声道丢掉或折到 stereo）
- 采样率不同：线性插值，状态留在 capture 线程

MVP 不引入 FFmpeg / rubato。

### 4.4 音量

`AtomicU32` 存 `f32.to_bits()`。render 输出按进程、物理麦、SFX、总音量缩放；权威值在 `AudioEngine`，不在 Slint。

---

## 5. 状态

运行时音频权威在 `AudioEngine`：输出、输入源、状态、电平、试听与文件播放控制。
持久化权威在 `AppConfig`。`AppState` 由桥接层持有；`UiCache` 仍包含部分配置镜像字段。
Slint 属性只作显示投影，不持有 PCM；缓存与配置的单一权威收口仍未完成。

UI 短句示例：

- `无法连接到音频设备`
- `进程已退出`
- `无法捕获该进程的音频`
- `设备可能已经被拔出。`

详细 HRESULT 走 tracing，不进音频线程热路径。

---

## 6. 错误

```rust
enum Error {
    Com(windows::core::Error),
    ProcessNotFound(u32),
    DeviceNotFound,
    CaptureInit(&'static str),
    RenderInit(&'static str),
    Format(&'static str),
    Io(std::io::Error),
    TimedOut(&'static str),
    InvalidArgs(String),
    Cable(String),
}
```

用户路径与音频循环禁止 `unwrap` / `expect` / `panic`。  
COM 失败映射为上述类型。

---

## 7. 依赖审查

| crate | 理由 | 能否 Win32 替代 | 体积 | 结论 |
|---|---|---|---|---|
| `windows` 0.62.2 | WASAPI / Win32 / Media Foundation / ToolHelp | 手写 FFI 更差 | 编译期为主 | **要** |
| `windows-core` 0.62.2 | COM / GUID / IUnknown 类型 | 需与 windows-rs 对齐 | 小 | **要** |
| `slint` + `slint-build` | 原生 UI，无 WebView | 纯 Win32 对话框开发成本高 | 依赖中最大 | **要** |
| `tracing` + `tracing-subscriber` | 结构化诊断日志 | `eprintln` 能替代但不可配置 | 小 | **要** |
| `serde` / `serde_json` | AppConfig 本地 JSON 存储与迁移 | 手写序列化易错 | 小 | **已使用** |
| `clap` / `tokio` / `cpal` / `rodio` / 其他大型音频框架 | 无必要 | 标准库 / Win32 已覆盖 | 避免新增 | **不要** |

Slint 1.18 features（正式构建）：`std`、`backend-winit`、`renderer-software`、`compat-1-18`。开发命令可额外启用官方 `live-preview` feature 和构建环境变量；发布不启用。

Release profile：

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

Step 9 对比 `opt-level = "z"`；若体积收益小、CPU 变差则维持 `3`。

---

## 8. CLI（无 UI 验证）

无额外 CLI 库，入口位于 `src/cli/mod.rs`，参数由 `std::env::args` 解析：

```text
mintcat-proc-mic.exe
--help
--list-processes
--list-devices
--capture <pid> --wav <path> --seconds N
--route <pid> --device <id> --seconds N
--snapshot-ui --out <path> [--menu]
--probe-playback <file> --seconds N --start N --out <path> [--sfx] [--raw]
```

`--capture` 和 `--route` 用于实机采集/路由；`--probe-playback` 用于离线逐采样探针。

---

## 9. UI

界面入口和分层：

```text
ui/app-window.slint
├─ pages/route.slint
├─ pages/player.slint
├─ pages/settings.slint
└─ overlays/context-menu.slint（只实例化一次）
```

三页为路由、音频库、设置。`AppHeader` 展示实时 EngineStatus；常驻状态栏承载全局混麦、主音量和启停。右键菜单组件负责遮罩与坐标钳制，页面只上报锚点和开关。

- 路由页从桥接层真实源模型展示进程、当前文件和物理麦；进程/文件/麦音量通过 AudioEngine API 更新。
- 路由源集合横向占满内容区，纵向由 `ScrollView` 承载；顶部添加源按钮打开小型对话框，进程直接添加，音频库入口复用条目菜单添加到路由主页。
- 路由源行输出下拉沿用当前全局多输出目标，不代表每源独立 render 路由。
- 音频库为分类侧栏、名称搜索、文件表格、选择详情和底部播放条；展示本地条目分类、格式、时长和导入状态。
- 分类路径使用 `/`；bridge 搜索过滤后的显示索引映射回配置条目。
- `AudioEngine::file_state()` 提供名称、时长和位置；UI 每 50 ms 更新显示，不接收 PCM。
- 播放目标、本机试听、播放进度和文件循环重启均调用现有 AudioEngine/bridge 逻辑。
- `OutputConfig.device_ids` 保存多选 render 端点；显示使用实时 FriendlyName，刷新通过 ID 恢复选择。
- 设置页分组为常规、音频设备、快捷键、虚拟线缆和关于；未实现的启动、托盘与主题选项不展示。
- S6 移除了未接线的全局播放按键、条目热键和热键模式设置。


---

## 10. 进程枚举

`CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)`。  
标题：`EnumWindows` + `GetWindowThreadProcessId` + `GetWindowTextW`，仅可见顶层。

跳过 PID 0/4、空名。不按 DRG 过滤。不硬编码游戏名。

---

## 11. 模块边界

```text
ui/ ──► ui_bridge/ ──► engine/AudioEngine ──► audio/
              ├────► library.rs / config/
              ├────► process / hotkey
              └────► audio::device/policy / dialogs / vbcable
process ─────► platform::com ◄──── audio::com
```

`engine` 和 `audio` 不引用 Slint；`process` 不再依赖 `audio`。UI bridge 不直接调用
`audio::decode`、`audio::sfx` 或 `audio::preview`；设备策略、文件对话框和 VB-CABLE 的
基础设施直连仍待进一步收口。

---

## 12. MVP 验收（Step 8 结束必须满足）

```text
启动 → 看到进程 → 选一个正在出声的进程
     → 看到设备 → 选目标
     → 开始 → 目标设备听到该进程
     → 音量有效
     → 停止后无声
```

额外手动：进程退出、无音频进程、高音量、短时间连续启停。  
热插拔自动恢复、开机启动、托盘：第二阶段。

---

## 13. 明确不做（当前阶段）

Electron / Tauri / WebView / FFmpeg / GStreamer / SQLite / 网络 API / 登录 / 自动更新 / 插件 / 自研虚拟声卡 / DRG Mod / TTS / 命名管道。

---

## 14. 工具链（本机）

已核实，不是计划：

| 项 | 值 |
|---|---|
| rustc | 1.98.1 (`stable-x86_64-pc-windows-msvc`) |
| rustup | 1.29.1 |
| 目标 | `x86_64-pc-windows-msvc` |
| MSVC | VS Community 2022 17.14，`cl` 19.44.35217，`D:\Program Files\Microsoft Visual Studio\2022\Community` |
| SDK | `D:\Windows Kits\10` `10.0.26100.0` |
| Edition | 2024（1.98 支持） |

Step 2 是历史基线；当前代码结构见根目录 `ARCHITECTURE.md`。
重构状态和未完成验收见 `PROJECT.md` 与 `docs/refactor-plan.md`。

# ProcessMic 代码重构计划（工程标准化）

状态：S1–S6 均有独立提交，但完整验收未完成；S0 资源基线、分类菜单快照和桌面音频验收仍缺。
依据：`AGENTS.md`（约束）、`goal.md`（权威需求）、`docs/step-01-architecture-analysis.md`、
`docs/step-02-rust-architecture.md`、`docs/ui-redesign-proposal.md`，以及本轮对当前工作树
**34 个源文件 / 8306 行**的逐文件审计（事实与行号见第 1 节，全部可在当前工作树复现）。

本轮追加的原则（你的原话）：`AGENTS.md` 里**不合理的规则可以打破**；彻底重构；
按工程标准模块化；**不要所有东西都写在一个文件里**。第 2 节就是据此给出的规则修订清单。

---

## 0. 摘要

现状三句话：

1. **规模已失控**：`src/ui_bridge.rs` 单文件 1976 行，其中一个函数 `wire_callbacks` 987 行、
   装了 38 个回调；`audio/capture.rs` 569、`engine.rs` 551、`ui/main.slint` 508、`config.rs` 421。
2. **分层只在名义上存在**：桥接层直接 import `audio::decode` / `audio::sfx` / `audio::preview`
   / `audio::device` / `audio::policy`，UI 自己持有一个第二音频引擎（`PreviewEngine`）；
   `process.rs` 反向依赖 `audio::com`；`audio` 层里散着 5 处别的职责。
3. **状态有多份权威**：`UiCache`（22 字段）与 `AppConfig` 有 10+ 组镜像字段，热键值双权威，
   音量三份（Slint 属性 + config + engine 原子），20 处回调各自`config::save` 写盘。

目标一句话：

```text
ui/(纯视图) → ui_bridge/(只翻译) → engine/(唯一音频权威) → audio/(WASAPI/MF 原语)
                                    ↑ config / platform / cli / dev（基础设施）
```

每个文件一个职责、单文件 ≤ 400 行、单函数 ≤ 100 行（硬阈值，本次重构的目标值 ≤ 250/≤ 60）、
依赖只向下、每个 `unsafe` 边界有 SAFETY 注释。

**不改**：目标平台、依赖集合（除删减）、UI 视觉语言、任何用户可感知行为（重构期间行为必须逐条保真）。

---

## 1. 审计事实（当前工作树）

### 1.1 规模与超限

| 文件 | 行数 | 问题 |
|---|---:|---|
| `src/ui_bridge.rs` | **1976** | `wire_callbacks` 987 行（38 回调）、`start_status_timer` 137 行、`snapshot_to_file` 46 行调试入口混在产物里 |
| `src/audio/capture.rs` | **569** | 6 类职责：COM 回调、资源所有权、进程环回激活、麦激活、实时循环、CLI WAV 导出 |
| `src/engine.rs` | **551** | 状态权威 + 线程工厂 + fan-out 调度三合一；`open_outputs` 单函数 ≈104 行 |
| `ui/main.slint` | **508** | 窗口 + 3 页面装配 + 状态栏 + 130 行音频库专属右键菜单层 |
| `src/config.rs` | **421** | 数据模型 + 持久化 + 单实例 + Slint 键位映射 四类职责 |
| 次高 | | `audio/decode.rs` 377、`audio/format.rs` 366、`main.rs` 316（其中 `probe_playback` 172 行） |

其余事实：

- 单行 > 100 字符：**全仓仅 `src/main.rs:140`（101 字符）**；`.slint`、`audio/`、`engine.rs` 全部合规。
- 单函数 > 100 行：`wire_callbacks` 987、`start_status_timer` 137、`on_start_stop` 闭包 138、
  `open_outputs` ≈104、`render.rs::run_render_loop` ≈123、`main.rs::probe_playback` 172。
- **仓库 0 个测试**（无 `#[test]` / `cfg(test)`）；`#[allow(...)]` 2 处；`tracing::*` 24 处。

### 1.2 分层与依赖方向

| 违规 | 证据 |
|---|---|
| 桥接层绕过领域层 | `ui_bridge.rs:13-17,124-125,274,283-295,441,500,1576,1620,1945-1946` 直接使用 `audio::{decode,sfx,preview,device,policy}` 与 `vbcable` |
| UI 持有第二个音频引擎 | `ui_bridge.rs:124` `Arc<Mutex<Option<PreviewEngine>>>`，播放/停止/音量全在回调里手工管 |
| 基础设施反向依赖领域 | `process.rs:19` `use crate::audio::com;`（COM 初始化应属共享基础设施） |
| 基础设施产出 UI 文案 | `config.rs:31 Hotkey::label()`、`process.rs::label()`、`config.rs:396 slint_key_to_vk()` |
| unsafe 越界 | `unsafe` 出现在 12 个文件：capture 28、render 11、process 7、decode 7、hotkey 6、buffer 5、device 5、config 4、policy 2、com 2、ui_bridge 2、main 1、format 1、vbcable 1 —— 白名单只列了 4 个文件，规则与实际严重脱节（见 §2） |

### 1.3 状态权威（同一份数据存多份）

| 数据 | 权威应当在哪 | 现状 |
|---|---|---|
| `audio_entries` / `audio_categories` | config | `UiCache` 也存一份，4 处手工同步 |
| `all_devices` / `set_default_mic` / `set_default_render` / `set_default_communications` | config | `UiCache` 各存一份 |
| 热键 `toggle_route` | config | `Rc<RefCell<config::Hotkey>>` + `config.hotkeys.toggle_route` 双写（565-572、577-594） |
| 三路音量 | engine（运行时） | Slint 属性 + `MixConfig` + engine 原子，三份 |
| 播放态 | engine/SFX | `UiCache.audio_playing/audio_anchor` + 4 个 Slint 属性双向同步 |
| `Hotkeys.mode` / `PadConfig` / `PlayerConfig` / `PlayMode` / `RouteConfig.auto_reconnect` | config | 只声明、全仓 0 使用点 |
| `UiCache.mic_ids/mic_names`、`pids` | — | 与 config 单向回写，退出后才一致 |

### 1.4 重复逻辑（含全部出现位置）

| 逻辑 | 次数 | 位置 |
|---|---:|---|
| 「可见下标 → 音频条目」查找表达式 | 9 | `ui_bridge.rs`（`audio_entry_matches` 9 个命中，其中 8 处为内联 `.iter().filter(...).nth(...)`） |
| 「取消解码 + SFX StopAll + preview 停源」停止播放序列 | 4 | `1093-1101`、`1107-1115`、`1149-1157`、`1195-1203` |
| 「状态复位 + 清 error」四连 | 4 | `1402-1429` |
| `config::save` 直接调用 | **20** | 全在 `ui_bridge.rs`，含音量拖动回调内（每次拖动事件写盘） |
| PCM↔f32 解码 | 3 | `format.rs:233`、`wav.rs:122`、`decode.rs:296` |
| f32→PCM 编码 | 3 | `format.rs:317`、`wav.rs:158`、`render.rs:202` |
| 声道映射 | 5 | `format.rs:282`、`sfx.rs:170`、`sfx.rs:245`、`preview.rs:113`、`preview.rs:217` |
| 线性重采样骨架 | 6 | `format.rs:134`、`format.rs:198`、`sfx.rs:162`、`sfx.rs:236`、`preview.rs:101`、`preview.rs:206`（sfx/preview 是最近邻，format 是插值） |
| 「lead ≈ 100ms」容量常量 | 4 | `engine.rs:438/447`、`sfx.rs:68/72`、`preview.rs:102/104`、`preview.rs:185` |
| 有界流泵 / 消费后 drain | 2 + 2 | `sfx.rs:218/256`、`preview.rs:192/227` |
| WASAPI 失效 HRESULT 分类 | 2 | `capture.rs:488`、`render.rs:227` |
| RIFF chunk 扫描 | 2 | `wav.rs:29`（游标）与 `wav.rs:93`（索引），各写一套 |

### 1.5 UI 回调里的阻塞工作

`engine.stop()`（join 全部线程）、`open_outputs`（COM + WASAPI + spawn）、`device::list_*`、
`policy::set_default_*`、模态文件对话框 `GetOpenFileNameW`、`decode::validate_file`（解码探测）、
`config::save`（写盘）都发生在 Slint 回调或 `run_ui` 启动路径上（`ui_bridge.rs:382,393,404,461,669,1221,1251-1258,1510-1511,1910`）。
`ui/main.slint` 无 `ListView`/`Flickable`，条目列表甚至**没有滚动容器**（`player.slint:143-195`），
条目多到超出容器即不可见——这是一个真实的可用性 bug。

### 1.6 音频热路径

合规面：`run_capture_loop` / `run_render_loop` / `run_fanout` / `run_sfx_loop` / `run_stream_source`
循环体内 0 把 Mutex、0 次 tracing、0 次文件/网络/UI；关停 80ms 超时、`Drop` join 齐全。

仍存在的隐患：`format.rs:242/291/190` `reserve`/`resize`（包 > 8192 时扩容）、`sfx.rs:222`
`extend_from_slice`、`decode.rs:353` 每块 `to_vec`、`buffer.rs:66` `push_paced` 用 `thread::sleep(2ms)`
轮询（最多阻塞 250ms）。这些不是崩溃级问题，但属于「实时线程不分配」原则的缺口。

### 1.7 unsafe

白名单（`capture.rs`/`render.rs`/`com.rs`/COM 回调/热键窗口过程）覆盖不到一半实际需要：
`buffer.rs` 的 SPSC 需要 `UnsafeCell`；`decode.rs`（Media Foundation）、`device.rs`（端点枚举）、
`policy.rs`（IPolicyConfig 手写 vtable）本质都是 COM/MF 调用。规则要改口径，不是把这些代码删掉（§2）。

### 1.8 死代码与残留（全仓出现次数已核对）

Rust 侧（1 次出现 = 只有定义、无调用点）：
`preview.rs:88 PreviewEngine::play`、`engine.rs:111 set_sfx_volume`、`engine.rs:547 copy_wave`（仅 1 个等价调用）、
`decode.rs:120 AudioReader::read_all_cancellable`、`device.rs:104 default_render_ids`、
`format.rs:176 Converter::passthrough`、`sfx.rs:270 _f32_format`（已 `#[allow(dead_code)]` 空实现）、
`capture.rs:102 MixFormat::as_ref`。
配置死模型：`config.rs:145 PlayMode`、`:210 PadConfig`、`:229 PlayerConfig`、`:53 Hotkeys.stop_all`、
`:112 RouteConfig.auto_reconnect`。
未接线的功能残迹（本次不实现，但必须诚实标注，不许留「能设置、不生效」的控件）：

- `Hotkeys.mode` / `HotkeyModeConfig`：**全仓无读取点**，模式在 `ui_bridge.rs:1356` 硬编码为 `HotkeyMode::System`。
- `PlaybackKeyMode`（设置页「不按键 / 播放开始按一次 / 播放期间一直按」）：只落配置，
  全仓无 `SetWindowsHookEx`、无注册点 → 设置后不生效。
- `AudioEntry.hotkey`（`ui_bridge.rs:563` 写入、`:979` 清除）：同样只落配置，无注册点 → 音频条目热键永不触发。
- 全仓**唯一真正注册**的热键是 `toggle_route`（`hotkey.rs:166-167`）。

Slint 侧：`ui/components/empty-state.slint` 全仓无引用；
`main.slint` 61 个 `in-out` 属性 / 40 个回调里，`mic-volume`、`mic-wave`、`mic-name`、`has-mic`、
`category-edit-name`、`duration-seconds`、`entry-edit-name`、`device-index` 页面体内零引用，
`open-settings`、`entry-edit-tags`、`entry-save-tags`、`mic-volume-edited` 无触发点。

工作树残留（未跟踪、未忽略）：`snap-*.raw`、`snap-menu*.png`、`probe-*.wav{,.ref.wav}`、
`seek-*.wav{,.ref.wav}`、`DEBUGGING.md`、`FRONTEND.md`；`capture.wav`/`target-relink/` 已被 gitignore。

### 1.9 文档漂移（`step-02-rust-architecture.md`）

1. §7 依赖表写「serde/serde_json：MVP 不引入」——已引入并在用。
2. §6 列 8 个 `Error` 变体——实际 10 个。
3. §2 目录树缺 `config.rs`/`hotkey.rs`/`vbcable.rs` 与 5 个 `audio/*`。
4. §8 CLI 清单缺 `--help`/`--snapshot-ui`/`--probe-playback` 及其子参数。
5. §3 线程/ring 模型写「3 条 ring + 单 render 线程」——实际源侧 6 条，每个输出目标另有 6 条 + 各自 render 线程 + fanout 线程。
6. §5 状态写单设备 `selected_device_id`——实际 `open_outputs(&[String])` 多输出。
7. §4.4 音量写三路（进程/麦/总）——实际四路（另有 `sfx`）。
8. §11 模块边界写「HRESULT 不出 audio」——`ui_bridge` 直接用了 Win32 对话框与 `process` 依赖 `audio::com`。

### 1.10 另两份文档的事实错误（本次已就地改写）

`FRONTEND.md`：

- 称 `ui/main.slint`「只负责窗口、页签、属性绑定和回调转发」——实际还装着 **130 行右键菜单覆盖层**
  （378-507）与状态栏里的路由控件（混入麦勾选、总音量、开始/停止）。
- 称右键菜单在 `ui/pages/player.slint`——实际菜单本体在 `main.slint` 顶层，页面只保存坐标与开关。
- 称「UI 不持有 PCM 数据」——表面上对（只有 32 点峰值历史），但同时说漏了 UI 层自己持有
  `PreviewEngine` 与解码线程、并直接调用 `audio::{decode,sfx,preview}` 这一实质越层。

`DEBUGGING.md`：

- 称「原生 Slint 窗口不能验证，只能编译 + 设备枚举 + 进程启动做冒烟」——**错**，
  本仓有 `--snapshot-ui`（软件渲染器出真 RGBA 图），菜单与索引列就是靠它核对的。
- 引用了不存在的属性名 `audio-duration-hns`（实际是 `audio-duration-seconds`）。
- 完全没提 `--probe-playback`——它是播放/解码类 bug 的唯一逐采样验证器。
- 定位说明写死 `src/ui_bridge.rs` 单文件，S1 之后该路径变成目录。

两份文件已按上述事实重写（仍放在仓库根目录，作为常读的边界说明），并在 `AGENTS.md` 第 5 节第 6 条
加入「改动后同步这两份 + `step-02-rust-architecture.md`」的硬要求，避免再次漂移。

---

## 2. 规则修订（`AGENTS.md` 要改的部分）

原则：**能落地的规则才是规则**。现行规则里有 4 条已经与代码现实冲突，留着比删掉更糟——
下个会话会照着过期规则继续写。

> 本节 R1（unsafe 边界规则）、R3（硬指标）、R4 的 UI 侧部分与新增的 R5–R9 已**就地写入 `AGENTS.md`**
> （新增 3.5.1 UI 线程约束、3.8 依赖红线与单一权威、3.9 测试与门禁；第 2 节依赖白名单补 serde；
> 第 1 节重构轮次例外；第 5/6/7 节验收、陷阱与模板同步）。本表保留作为「为什么改」的记录。

| # | 现行规则 | 为什么不合理 | 改成 |
|---|---|---|---|
| R1 | 「`unsafe` 只放 `audio/capture.rs`、`render.rs`、`com.rs`、COM 回调、热键窗口过程」 | 文件白名单与语言现实冲突：SPSC ring 的本质就是 `UnsafeCell`；MF 解码、端点枚举、IPolicyConfig vtable 都是 COM/MF，不写 `unsafe` 做不了。按字面执行只能靠把 `unsafe` 塞进白名单文件来造假 | 改为**边界规则**：`unsafe` 必须封装在「一个模块即一个 unsafe 边界」内，模块对外只暴露安全 API；每处 `unsafe` 必须有 `// SAFETY:` 说明（别名/生命周期/谁释放/哪个线程） | 
| R2 | 「不引入多 crate、复杂架构」 | 这条本身合理，本次**保留**；但要澄清：单 crate ≠ 单文件。8.3k 行单 crate 完全可以、也应该用 `module/` 目录分层 | 明确写入：单 crate 不变；禁止把不同职责堆进同一个 `.rs`；目录即分层 |
| R3 | 「单文件超 400 行就该拆」 | 只有阈值、没有口径，实际被无视（1976 行的文件存在了多轮） | 补硬指标：单文件 ≤ 400 行、单函数 ≤ 100 行、单闭包 ≤ 30 行、`.slint` 单文件 ≤ 300 行；**每片重构必须让超限清单变短**；门禁脚本由 S0 建（`scripts/check.cmd`），建之前靠数行数自查 |
| R4 | 「音频热路径禁止…锁/分配」 | 方向正确，但没说 UI 侧。真正的阻塞源是 UI 回调 | 增补：**UI 回调内禁止同步 IO/解码探测/枚举/join**（>16ms 的工作一律丢线程或派发）；`config::save` 必须 debounce（≥300ms），禁止在拖动类回调里逐事件写盘 |

新增规则（本次重构必须落地）：

- R5 依赖方向：`ui_bridge` 不得 `use crate::audio::{decode,sfx,preview}`；UI 不得持有音频引擎；
  工具校验：`scripts/check.cmd` 用 `findstr` 做红线检查。
- R6 单一权威：`AppConfig` 是持久化权威、`AudioEngine` 是音频运行时权威；
  UI 缓存只允许存「派生/视图态」（下标、筛选词、菜单坐标）。
- R7 测试：纯逻辑（PCM 转换、WAV 解析、分类路径、时长格式化、键位映射、配置迁移）必须有单元测试；
  这是本仓库第一次引入测试，范围仅限无 IO 的纯函数。
- R8 每片验收：`scripts/check.cmd`（fmt --check + `clippy -- -D warnings` + build + 行数门禁）+
  `scripts/smoke.cmd`（设备、进程与当前支持的 UI 快照）+ 原生窗口手工路径。
  性能、采样和截图基线只记录已实测数据；未覆盖状态须明确列出。
- R9 文档同步：`AGENTS.md` 要求按事实更新根目录 `ARCHITECTURE.md`、`BACKEND.md`、`API.md`、
  `FRONTEND.md`、`FILE-CONVENTIONS.md`、`DEBUGGING.md` 和 `PROJECT.md`；旧阶段架构文档同步变动的历史事实。

`goal.md` 不改：本次没有触碰它的任何禁令（无 FFmpeg/网络/数据库/自研驱动），也没有改产品方向。

---

## 3. 目标结构

### 3.1 Rust

```text
src/
  main.rs                    ≤ 60   仅：DPI + tracing + 派发（组合根）
  cli/
    mod.rs                   ≤ 80   argv → 命令分派、--help
    list.rs                  ≤ 80   --list-processes / --list-devices
    capture.rs               ≤ 90   --capture（调 audio::dump）
    route.rs                 ≤ 90   --route（调 engine 安全 API）
  dev/
    mod.rs                   ≤ 40   dev-only 工具（不进 UI 路径）
    probe.rs                 ≤ 200  probe_playback（解码/时钟比对验证器）
    snapshot.rs              ≤ 90   --snapshot-ui（原生窗口截图核对）
  ui_bridge/                 桥接层：只做「Slint 属性 ⇄ 领域调用」翻译
    mod.rs                   ≤ 120  窗口创建、装配、事件循环、退出收尾
    state.rs                 ≤ 150  AppState（engine/cache/config/monitor/hotkeys 单一持有）
    cache.rs                 ≤ 120  UiCache：只留派生/视图态
    callbacks/
      mod.rs                 ≤ 60
      route.rs               ≤ 250  进程/设备/音量/启停
      library.rs             ≤ 250  分类树与条目 CRUD
      player.rs              ≤ 250  播放/进度/停止
      settings.rs            ≤ 220  设备/热键/预览/Cable
    lists.rs                 ≤ 220  refill_processes/mics/devices + label 文案
    persist.rs               ≤ 180  restore/persist + debounce save
    input.rs                 ≤ 120  Slint 键位 ↔ VK 映射、热键捕获
    window.rs                ≤ 120  尺寸适配、工作区
    format.rs                ≤ 120  纯函数：时长/位置/分类显示（带单测）
  engine/                    领域层：唯一音频权威
    mod.rs                   ≤ 200  AudioEngine 状态 + 安全 API
    output.rs                ≤ 200  open_outputs/stop（拆成 open/build/spawn/commit）
    sources.rs               ≤ 220  进程源 / 麦源 生命周期
    fanout.rs                ≤ 200  FanoutTarget/run_fanout/spawn_target_render
    monitor.rs               ≤ 160  本机试听（原 PreviewEngine，收归引擎）
    playback.rs              ≤ 220  文件播放：play/stop/seek/progress（取代 UI 手工管线程）
  library.rs                 ≤ 220  音频库领域：分类路径语义、CRUD、时长补全（带单测）
  audio/                     WASAPI/MF 原语：一个模块一个 unsafe 边界
    mod.rs                   ≤ 30
    com.rs                   ≤ 60   COM apartment
    buffer.rs                ≤ 140  SPSC ring（唯一允许 UnsafeCell 的边界）
    format.rs                ≤ 160  AudioFormat / WAVEFORMATEX 解析
    pcm.rs                   ≤ 200  PCM↔f32、声道映射（合并 5 处重复）
    resample.rs              ≤ 140  线性重采样（合并 6 处骨架）
    runtime.rs               ≤ 160  lead/容量公式、有界泵、drain（合并 4+2+2 处）
    errors.rs                ≤ 80   WASAPI HRESULT 分类（合并 2 处）
    capture/
      mod.rs                 ≤ 120  打开工厂
      session.rs             ≤ 140  CaptureClient/MixFormat 所有权与 Drop
      process_loopback.rs    ≤ 160  ActivateAudioInterfaceAsync + VT_BLOB
      mic.rs                 ≤ 100  物理麦激活
      loop_thread.rs         ≤ 160  实时捕获循环
    render.rs                ≤ 200  渲染设备 + 实时循环（拆开打开与循环）
    device.rs                ≤ 180  端点枚举
    vdevice.rs               ≤ 120  虚拟驱动识别/配对启发式（纯字符串规则）
    policy.rs                ≤ 80   IPolicyConfig
    peak.rs                  ≤ 40
    wav.rs                   ≤ 140  RIFF 读（一套扫描）+ 写出
    decode.rs                ≤ 240  Reader 抽象 + MF 实现
    sfx.rs                   ≤ 200  声板混音线程与播放器
    dump.rs                  ≤ 120  CLI WAV 导出（从 capture.rs 移出）
  config/
    mod.rs                   ≤ 180  AppConfig 数据模型（纯数据 + Default）
    store.rs                 ≤ 120  load/save/路径/旧版迁移
    single_instance.rs       ≤ 60
  platform/                  跨层共享的 Win32 基础设施
    mod.rs                   ≤ 30
    com.rs                   ≤ 40   转发/别名 audio::com（供 process/engine 用，消除反向依赖）
    dpi.rs                   ≤ 60   进程 DPI 感知
    dialogs.rs               ≤ 90   文件选择对话框
    workarea.rs              ≤ 60   主屏工作区
  process.rs                 ≤ 200  进程枚举（去掉 UI 文案与对 audio 的依赖）
  hotkey/
    mod.rs                   ≤ 120  RegisterHotKey 线程
    hook.rs                  （留白：全局钩子属于既有提案 S6，不属本次）
  vbcable.rs                 ≤ 200
  error.rs                   ≤ 90
docs/
  refactor-plan.md           本文件
```

### 3.2 Slint

```text
ui/
  app-window.slint           ≤ 200  窗口、顶栏、页签、页面容器、状态栏（仅剩下发给页面的绑定）
  theme.slint                ≤ 90   颜色 + 字号/字重/间距/尺寸 token（补齐缺失 token）
  overlays/
    context-menu.slint       ≤ 130  ContextMenu（items 模型 + 坐标 + 内部钳制），取代 main.slint 里 130 行菜单层
  components/
    button.slint card.slint tab-bar.slint menu-item.slint slider-row.slint
    hotkey-field.slint level-meter.slint
    section-title.slint      ≤ 25   区块标题（现重复 ~11 处）
    hint-text.slint          ≤ 20   提示文字（现重复 8 处）
    field.slint              ≤ 40   只读字段（route 页局部组件与 hotkey-field 的重复外观）
    list-row.slint           ≤ 45   列表行（分类行/条目行/设备行同一结构，现重复 3 处）
    checkbox-row.slint       ≤ 30   勾选项（现重复 6 处）
    spacer.slint             ≤ 8    撑开占位（现重复 5 处）
  pages/
    route.slint settings.slint player.slint   （各自 ≤ 250，只组合组件，不再复制结构）
```

### 3.3 依赖方向

```text
ui/ (Slint)  ──属性/回调──►  ui_bridge/  ──安全 API──►  engine/  ──►  audio/
                                  │                        │
                                  └──►  library/  ──►  config/        （数据）
                                  └──►  platform/（com/dpi/dialogs/workarea）
cli/ ──► engine/ + audio/（验证路径，不经 ui_bridge）
dev/ ──► 任意（仅调试）
```

红线（可机器校验）：`ui_bridge/` 不得出现 `audio::decode|sfx|preview`；
`process.rs` 不得出现 `crate::audio::`；`config/` 不得出现 Slint 类型；
`engine/`、`audio/` 不得出现 Slint 类型。

---

## 4. 实施切片

顺序有依赖，逐片执行、逐片验收、逐片可回退。每片一个提交（或一组小提交）。

### S0 — 基线固化与清理（不改行为）

- 已完成：`scripts/check.cmd`、`scripts/check-lines.ps1`、`scripts/smoke.cmd` 和调试产物忽略规则。
- 已完成：S0 之后各源代码切片分别提交；当前门禁包含严格 Clippy、构建和行数检查。
- 未完成：debug/release 体积、RAM、CPU、启动时间的实测基线；未生成 `step-33-refactor-baseline.md`。
- 未完成：分类菜单快照。现有 `--snapshot-ui` 只覆盖默认态和条目菜单两种状态。
- 验收状态：check/smoke 自动路径通过；资源数据和三态截图基线尚未完成。

### S1 — 桥接层目录化（`ui_bridge.rs` 1976 → 目录，最大收益）

- 把 987 行 `wire_callbacks` 拆到 `callbacks/{route,route_start,library,library_entries,library_metadata,player,settings}.rs`。
- 新增 `AppState`，将回调入口收敛为 `(&MainWindow, &AppState)`；列表、状态、持久化、窗口与播放辅助各自独立。
- `library::visible_entry()` 统一可见下标映射；`player::stop_playback()` 统一解码取消、SFX StopAll 与试听停止。
- `persist::SaveQueue` 使用单次 300 ms 防抖与 FIFO 单写线程；18 个回调写盘点改为调度，退出时 flush/join。
- `snapshot_to_file` 迁到 `dev/snapshot.rs`；`probe`/CLI 相关未动。
- 验收目标：`check.cmd`、`smoke.cmd`、UI 路由与音频库完整手工路径、截图基线一致。
- 实测状态：自动门禁和默认/条目菜单快照通过；桌面音频路径与分类菜单快照尚未覆盖。

### S2 — 领域门面：UI 不再绕过引擎

- 已完成：`engine/monitor.rs` 收归试听目标与音量，UI 只调 `AudioEngine` 安全 API。
- 已完成：`engine/playback.rs` 收归文件播放、取消、解码流、SFX/试听分发和播放状态。
- 已完成：根 `library.rs` 集中分类路径语义、递归重命名/删除、可见索引与文件验证，含单测。
- 已完成：`ui_bridge/` 不再直接引用 `audio::decode`、`audio::sfx`、`audio::preview`。
- 未完成：`seek_file(hns)` 尚以停止后重新 `play_file` 实现，S4 音频原语拆分未开始。
- 验收：构建、2 个领域单测、设备/进程/UI 快照冒烟通过；真实桌面音频交互未自动化。

### S3 — Slint 结构重构 + 契约收窄

- 已完成：窗口入口迁移为 `ui/app-window.slint`，右键菜单覆盖层迁移为 `ui/overlays/context-menu.slint`；菜单遮罩、坐标钳制和动作收在组件内。
- 已完成：按《重构ui.md》统一全局 Windows 10 视觉 token，重排路由、音频库和设置页；保留既有属性、回调和业务入口。
- 已完成：音频库条目列表加入 `ScrollView`，分类空白区域和分类行右键快照均通过。
- 未完成：`ListView` 虚拟化、通用 `list-row` 等列表组件、`empty-state` 接入。
- 验收：Slint 构建、CLI/UI 快照冒烟通过；真实设备路由、试听听感和桌面鼠标交互未自动化。

- `main.slint` → `app-window.slint`（≤200）；130 行菜单层 → `overlays/context-menu.slint` 组件
  （`items` 模型 + 坐标 + 内部钳制，消灭 11 处 `xxx-menu-open = false` 与硬编码 132px/292px）。
- 音频库专属属性/回调（9 个只被顶层菜单触发的回调）从窗口收拢到页面内。
- `theme.slint` 已补齐字号、颜色、圆角、间距、控件尺寸和状态色 token；通用结构组件仍待后续切片。
- 音频库条目列表已加 `ScrollView`；`ListView` 虚拟化和 `list-row` 统一组件仍未完成。
- 删除 §1.8 的 11 个死属性/死回调，并同步删 `ui_bridge` 里对应注册。
- 验收：`--snapshot-ui` 三种状态（列表/分类菜单/条目菜单）截图 + 手工 5 条主路径；
### S4 — 音频与引擎按职责拆文件

- 已完成：多输出 fan-out、格式转换与目标 render 工厂迁移到 `engine/fanout.rs`。
- 已完成：CLI Process Loopback WAV 导出迁移到 `audio/dump.rs`。
- 未完成：`engine.rs` / `capture.rs` 目录化、PCM/resample/runtime/errors 原语合并和实时分配收敛。
- 验收：构建、领域单测、设备/进程/UI 快照冒烟通过；真实 WAV/route 采样验收未自动化。


- `audio/capture.rs`(569) → `capture/`；`CLI WAV 导出` → `audio/dump.rs`。
- `engine.rs`(551) → `engine/{mod,output,sources,fanout}`；`open_outputs` 拆成
  `open_clients` / `build_source_rings` / `spawn_targets` / `spawn_fanout` / `commit_state`。
- 抽 `audio/{pcm,resample,runtime,errors}.rs`，合并 §1.4 的解码/编码/声道/重采样/常量/泵/错误码重复。
- `format.rs` → `format.rs` + `pcm.rs`；`device.rs` → `device.rs` + `vdevice.rs`；`wav.rs` 一套 chunk 扫描。
- 顺带修 §1.6 的实时线程分配：`format.rs` 的 `reserve` 改为启动期一次性分配 + 上限丢弃；
  `buffer.rs::push_paced` 的 `thread::sleep` 换成事件/自旋混合（保持 100ms 语义）。
- 验收：`--capture` 产 WAV 与原参考逐采样一致（沿用 `probe_playback` 的比对思路）；
  `--route` 有声；ring 数量/线程数与改造前一致（`tracing` 打印核对）。

### S5 — 入口与基础设施

- 已完成：CLI 派发、帮助、设备/进程/capture/route 和 probe 迁移到 `src/cli/mod.rs`，`main.rs` 收敛为组合根。
- 已完成：新增 `platform/com.rs`，`process.rs` 不再直接依赖 `audio::com`。
- 已完成：config 拆为 model/store/single-instance；CLI 与 COM 转发已迁移。
- 未完成：`Hotkey::label()` / `slint_key_to_vk()`、`process::label()` 和 dialogs/workarea 的基础设施归位。
- 验收：构建、设备/进程 CLI 与 UI 快照冒烟通过。
- 未完成：`probe_playback` 仍在 `cli/mod.rs`；Win32 对话框、工作区查询和热键捕获仍在桥接层。
- 未完成：把 `slint_key_to_vk()` / `Hotkey::label()` / `ProcessInfo::label()` 移到桥接层。
- 未完成验收：逐字比较 CLI 输出、旧配置迁移兼容与单实例双开退出。

### S6 — 收口：死代码、可见性、测试、文档与规则

- 已完成：config 拆为 model/store/single-instance；移除未接线的 Pad/播放器字段、热键模式/停止热键、自动重连和播放按键设置。
- 已完成：删除已核实未调用的 SFX/试听/解码辅助；`scripts/check.cmd` 的严格 Clippy 与构建通过。
- 已完成：分类索引与递归删除单测；`scripts/smoke.cmd` 的设备/进程和两种 UI 快照通过。
- 未完成：将 `Hotkey::label()` / `slint_key_to_vk()` / `ProcessInfo::label()` 移出基础设施，收紧剩余 `pub` 可见性。
- 未完成：PCM/WAV/config 迁移/时长格式化等单测；当前只有两条 library 单测。
- 未完成：零超限文件预算；`engine.rs`、`audio/capture.rs`、`ui/app-window.slint` 仍由行数门禁按不增长基线豁免。
- 未覆盖：真实设备 WAV 逐采样/route 听感、Windows 桌面交互和 RAM/CPU/启动时间基线。

---

## 5. 删除清单（已核对，非猜测）

代码：`PreviewEngine::play`、`AudioEngine::set_sfx_volume`、`copy_wave`、`AudioReader::read_all_cancellable`、
`device::default_render_ids`、`Converter::passthrough`、`sfx::_f32_format`、`MixFormat::as_ref`、
`config::{PlayMode, PadConfig, PlayerConfig, Hotkeys.stop_all, RouteConfig.auto_reconnect}`（若 S6 决定不做声板）、
`ui/components/empty-state.slint`（或按 §S3 真正启用）。
文件：根目录 `snap-*.raw`、`snap-menu*.png`、`probe-*.wav*`、`seek-*.wav*`。
保留但迁移：`--snapshot-ui`、`--probe-playback`（本仓唯一的 UI/音频自动验证手段，移到 `dev/`）。

---

## 6. 测试与验收

| 类型 | 范围 | 手段 |
|---|---|---|
| 单元 | 纯函数：PCM/WAV/分类路径/时长格式化/键位映射/配置迁移 | `cargo test`（本次首次引入，仅无 IO 逻辑） |
| 门禁 | 格式、lint、行数、依赖红线 | `scripts/check.cmd` |
| 冒烟 | 设备/进程枚举、截图 | `scripts/smoke.cmd` |
| 人工 | 每轮验收：选进程→选设备→开始→有声→音量→停止；音频库播放/切歌/进度/菜单 | 原生窗口操作 |
| 基线对比 | 截图、CLI 输出逐字、WAV 逐采样、ring/线程数、RAM/CPU/体积 | 与 S0 基线对比 |

每片结束必须写 `docs/step-XX-*.md`（做了/没做/怎么跑/实测），并同步被改掉的架构事实。

---

## 7. 风险与取舍

| 风险 | 说明 | 应对 |
|---|---|---|
| 大重构改出行为回归 | 8.3k 行全动一遍 | S0 基线 + 每片截图/CLI/WAV 对比；切片间不并行改同一文件 |
| 拆分过程中编译长期不绿 | 「一半在旧文件、一半在新文件」 | 每片结束时 `check.cmd` 必须全绿；片内允许中间态，片尾不允许 |
| 拆文件可能引入性能变化 | 模块边界不影响热路径，但要防新分配 | 热路径只做移动不做改写；S4 单独复核 ring/线程数 |
| `ListView` 改变交互细节 | 虚拟化可能影响选中/右键坐标 | S3 单独验证菜单坐标与选中态；坐标改用组件上报 |
| debounce 保存丢最后一次修改 | 退出时未 flush | `persist.rs` 在 `run_ui` 退出路径强制 flush（保留现有退出保存语义） |
| 不拆多 crate 是否够「工程标准」 | 8.3k 行单 crate 是可接受的工程做法；拆 crate 会带来 workspace、构建脚本、feature 传递的复杂度，收益仅为「更严格边界」 | 本次**不拆**；若后续超过 ~20k 行或需要单元测试复用，再评估 `audio` 独立 crate |

---

## 8. 范围边界

- **本次不做**：新功能（全局钩子实现、托盘、开机启动、自动重连、声板 pad 网格）、
  新依赖、UI 视觉重设计、多 crate 拆分、`goal.md` 目标变更。
- 已有提案 `docs/ui-redesign-proposal.md` 的 S1–S5 事实上已在工作树实现（三页签、音频库、
  MF 解码、多输出、单实例）；未实现的 S6（链路钩子）与 S7（性能实测）保持独立，
  它们要用的模块位置由本计划预留（`hotkey/hook.rs`、`engine/monitor.rs`）。
- 重构期间每个提交都必须是「可运行 + 行为不变」，不允许搭脚手架式的大爆炸切换。

---

## 9. 附录：本轮审计命令（可复现）

```text
行数/超长行：逐文件 len(lines) 与 max(len(line))，唯一 >100 行为 src/main.rs:140
ui.on_ 回调数：ui_bridge.rs 正则 ui\.on_[a-z_0-9]+ → 38
config 写盘：config::save( 全仓 20 处，均在 ui_bridge.rs
条目查找重复：audio_entry_matches → 9 处
停止播放重复：SfxCommand::StopAll → 4 处；set_player_playing(false) → 5 处
契约面：ui/main.slint in-out property 61、callback 40
测试：#[test] / cfg(test) 全仓 0 处
死代码：上列符号全仓出现次数 = 1（仅定义）
unsafe：按文件计数（capture 28 / render 11 / process 7 / decode 7 / hotkey 6 / buffer 5 / device 5 / …）
```

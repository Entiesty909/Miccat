# UI Bridge 与音频模块大型收口重构计划

状态：计划已批准，开始执行；本文件先于代码重构写入，作为本轮范围和验收契约。

范围：在保持用户可感知行为、Slint callback 名称、配置兼容和音频线程模型不变的前提下，收口 `ui_bridge` 残留职责，统一源行投影，清理桥接状态镜像，拆分开发快照与音频模块，并补齐关键注释。

明确不做：新产品功能、UI 视觉重设计、新依赖、多 crate、托盘、全局钩子、自动重连、声板网格、Deep Rock Galactic 特化、改变音频格式或路由语义。

## 一、当前问题

1. `src/ui_bridge/sources.rs` 使用多组并行 Slint 模型，生产路径和 `dev/snapshot.rs` 分别手工构造源行，存在字段长度不一致风险。
2. `src/ui_bridge/mod.rs` 同时承担组合根、独立窗口生命周期、窗口位置保存和子窗口同步。
3. `src/ui_bridge/dev/snapshot.rs` 同时承担场景构造、配置读取、鼠标事件模拟、诊断数据种子、渲染和文件输出；还直接依赖 bridge 私有实现。
4. `UiCache` 镜像部分 `AppConfig`，并保存部分音频运行态；需要区分持久化权威、引擎权威和纯视图态。
5. 状态定时器、路由回调和单独转发回调重复按源行数字类型计算进程/文件槽位。
6. `config::Hotkey::label()`、`config::slint_key_to_vk()`、`process::ProcessInfo::label()` 含有 UI 展示或输入映射职责。
7. `--snapshot-ui` 使用真实配置作为 fixture，并在补全音频时长后写盘；快照命令不应继续扩大对生产配置的耦合。
8. `engine.rs`、`audio/capture.rs` 及相关音频原语职责仍然偏宽，PCM、重采样、运行时容量/泵、HRESULT 分类等存在收口空间。
9. Slint 源行使用并行数组，必须先建立唯一 Rust 投影接缝，再评估是否收窄 Slint 属性契约。
10. 实时音频路径存在启动后缓冲扩容、块复制和节拍等待等待复核项；只能在有基线和逐采样/路由验证后处理。

## 二、执行切片

### R1 — 源行投影与快照 fixture 统一

涉及：

- `src/ui_bridge/sources.rs`
- `src/ui_bridge/dev/snapshot.rs`
- 必要的 `src/ui_bridge/status.rs`
- 必要的 `src/ui_bridge/callbacks/route/sources.rs`
- 必要的 `src/ui_bridge/callbacks/route_start/source_only.rs`

动作：

- 为进程、音频文件、物理麦克风提供统一的 `SourceRows` 构造入口。
- 保留 Slint 现有并行属性和 `0=进程、1=文件、2=麦克风` 协议，先不改 UI 契约。
- `SourceRows::apply()` 继续负责模型差异比较和模型写入。
- 快照不再手工追加不完整的源行字段。
- 集中源类型、源槽位和源行长度不变量的内部辅助逻辑。
- 为纯映射和模型长度不变量增加测试。

行为约束：源行顺序、文案、图标、输出下标、50 ms 动态更新、下拉框稳定性不变。

### R2 — `snapshot-ui` 场景模块化和副作用隔离

涉及：

- `src/ui_bridge/dev/snapshot.rs`
- `src/ui_bridge/dev/mod.rs`
- `src/cli/mod.rs`
- 需要时新增 `src/dev/`，但只有在不增加 bridge 私有接口泄漏时迁移。

动作：

- 将参数校验、快照上下文、页面 fixture、交互模拟、诊断 fixture、渲染和写出拆成窄函数。
- 将 CLI 参数结构与内部快照场景结构分离。
- 使用显式导入，删除通配导入造成的隐式依赖。
- 快照只在内存中补全测试数据；不因生成快照修改用户配置。
- 保留原始 RGBA 格式和所有现有 CLI 参数。

行为约束：默认页面、菜单断言、独立窗口重开断言和输出文件格式不变。

### R3 — `ui_bridge/mod.rs` 组合根收口

涉及：

- `src/ui_bridge/mod.rs`
- `src/ui_bridge/window.rs`
- 新增 `src/ui_bridge/window_lifecycle.rs`
- `src/ui_bridge/status.rs`
- `src/ui_bridge/callbacks/route.rs`

动作：

- 将窗口回调、窗口位置保存、显示/隐藏、子窗口同步迁移到 `window_lifecycle.rs`。
- `mod.rs` 只保留初始化、组合、事件循环和退出收尾。
- 直接迁移调用点，不保留旧接口转发壳。
- 为窗口生命周期关键分支补中文注释，说明 Slint 线程、位置持久化和重绘原因。

行为约束：窗口位置、图标、首次居中、隐藏/重开、主窗口退出保存行为不变。

### R4 — 状态权威和桥接缓存收口

涉及：

- `src/ui_bridge/cache.rs`
- `src/ui_bridge/state.rs`
- `src/ui_bridge/persist.rs`
- `src/ui_bridge/mod.rs`
- `src/ui_bridge/callbacks/**/*.rs`
- `src/config/mod.rs`
- `src/engine/**/*.rs`

动作：

- `AppConfig` 继续作为持久化权威，`AudioEngine` 继续作为运行时音频权威。
- 删除能由配置或引擎即时派生的 `UiCache` 镜像；保留筛选词、分页、可见索引、菜单坐标、窗口交互和列表快照等纯视图态。
- 对必须跨回调保存的槽位级派生状态，明确字段不变量和生命周期，不能仅为“看起来干净”强行删除。
- 将配置恢复和退出持久化集中到既有 `persist.rs`，保持 300 ms 防抖和退出 flush。
- 清理确实无调用点的字段和辅助函数，删除前查全仓引用。

行为约束：旧配置可迁移；进程/音频卡片独立音量、静音、输出和启用状态不变；不改变 JSON 字段含义。

### R5 — UI 文案和输入映射归位

涉及：

- `src/config/mod.rs`
- `src/process.rs`
- `src/ui_bridge/input.rs`
- `src/ui_bridge/lists.rs`
- `src/ui_bridge/library.rs`
- `src/cli/mod.rs`
- 相关测试和文档

动作：

- 将 `Hotkey::label()` 的 UI 展示逻辑迁移到 bridge/表示层；配置只保留数据。
- 将 `slint_key_to_vk()` 迁移到 bridge 输入适配层；配置不依赖 Slint 语义。
- 将 `ProcessInfo::label()` 拆为领域数据和调用方展示格式；CLI 与 UI 各自明确输出格式。
- 不改变 CLI 输出、人类可读文案和快捷键支持范围。
- 为键位映射和格式化逻辑补纯单元测试。

### R6 — `engine` / `audio` 按职责拆分

涉及：

- `src/engine.rs` 及其 `engine/` 子模块
- `src/audio/capture.rs`
- `src/audio/format.rs`
- `src/audio/sfx.rs`
- `src/audio/preview.rs`
- `src/audio/render.rs`
- 必要时新增 `audio/pcm.rs`、`audio/resample.rs`、`audio/runtime.rs`、`audio/errors.rs`、`audio/capture/`
- `src/cli/mod.rs`、相关调用点和架构文档

动作：

- 先做纯移动和可见性收口，再做重复原语合并；禁止边拆文件边改变音频算法。
- `engine` 按输出、源生命周期、fan-out、试听、文件播放区分职责。
- `audio` 按 WASAPI capture、render、PCM/声道、重采样、运行时容量、错误分类区分职责。
- CLI WAV dump 保持与音频捕获实现的安全接口，不反向依赖 UI。
- 每次目录化后立即构建，禁止长期保留双实现。

行为约束：采样格式、线程数量、ring 生产者/消费者关系、HRESULT 映射、停止和 join 语义不变。

### R7 — Slint 源行模型接缝收窄

涉及：

- `ui/app-window.slint`
- `ui/components/page-host.slint`
- `ui/components/source-row.slint`
- `ui/pages/route.slint`
- `src/ui_bridge/sources.rs`
- 相关回调和快照

动作：

- 先保留外部属性契约，使用 Rust 内部行模型统一生成并校验并行数组。
- 在快照和运行时共享同一投影适配器。
- 只有在 R1–R3 验证通过后，才评估是否能减少 Slint 属性数量；如果需要改变 callback/property 契约，另拆独立切片。
- 不把 `AudioEngine` 或 PCM 放入 Slint。

行为约束：UI 视觉、源行布局、下拉框、实时峰值和交互回调不变。

### R8 — 实时路径性能复核

涉及：

- `src/audio/format.rs`
- `src/audio/buffer.rs`
- `src/audio/sfx.rs`
- `src/audio/decode.rs`
- `src/audio/preview.rs`
- `src/audio/render.rs`
- `src/engine/fanout.rs`
- `src/engine/output.rs`

动作：

- 先记录现有 capture、fan-out、SFX、render 的缓冲容量、线程数量和行为基线。
- 将可预知容量的 `reserve/resize` 移到启动阶段；运行循环禁止扩容。
- 审核 `to_vec`、`extend_from_slice`、周期性 sleep 和 drain 逻辑，只处理能证明不改变时序/音质的项。
- 不在热路径增加日志、锁、文件、网络、UI 或新的分配。
- 使用 `--capture`、`--probe-playback`、`--route` 和必要的 WAV 逐采样对比验证。

行为约束：不能以降低音质、改变延迟、改变停止语义或吞掉错误换取性能数字。

## 三、注释规则

本轮碰到的前端和后端代码必须补齐关键注释：

- 模块顶部用 `//!` 说明职责。
- `pub`、`pub(crate)`、`pub(super)` 类型、字段、函数说明用途或不变量。
- 函数内部只给关键机制加注释，重点解释：
  - 为什么要保持模型指针不变。
  - 为什么某个字段属于缓存而不是权威状态。
  - 为什么某个线程/缓冲区不能分配或阻塞。
  - 为什么某个 COM/Win32 调用需要特定线程或释放方式。
  - 为什么窗口需要二次重绘或退出时 flush。
- 不添加复述代码表面语义的噪声注释。
- 注释使用中文；标识符和 API 名称保持英文。

## 四、验收顺序

每个切片都必须：

1. `scripts\\build-dev.cmd fmt -- --check`
2. `scripts\\build-dev.cmd build`
3. 纯逻辑变化执行 `scripts\\build-dev.cmd test`
4. 执行 `scripts\\smoke.cmd`
5. 运行对应 `--snapshot-ui` 快照
6. 音频路径变化执行 `--capture`、`--probe-playback` 或 `--route`
7. 更新 `ARCHITECTURE.md`、`BACKEND.md`、`FRONTEND.md`、`API.md`、`DEBUGGING.md`、`PROJECT.md` 中受影响事实

真实设备路由听感、桌面鼠标操作和 RAM/CPU/启动时间若未实际测量，文档中明确标记为未覆盖，不写成已验证。

## 五、回退和风险

- 每个 R 切片结束时保持可编译、可启动、可生成快照。
- 不并行修改同一个模块的生产实现。
- 纯移动与行为修改分开，先保证编译和快照，再处理性能。
- 若 Slint 契约收窄导致回调或布局风险，保留 R1 的内部适配器，不继续改变外部契约。
- 若实时性能改动无法通过 WAV/路由证据证明收益，保留原实现并记录原因。

## 六、当前交付状态

本文件已先于代码写入；R1–R5 的安全子切片和音频错误分类已执行，代码与实测结果见第七、八、九节。


## 七、已执行内容

已完成的行为保持型重构：

- `SourceRows` 改为私有并行模型，由 `ProcessRowInput`、`FileRowInput`、`MicrophoneRowInput` 三个输入结构统一构造；运行时和 `--snapshot-ui` 共用同一套行构造，并加入模型长度不变量检查与单测。
- `src/ui_bridge/dev/snapshot.rs` 不再为补全音频时长写回用户配置；快照写出独立为 RGBA 缓冲适配器。
- `src/ui_bridge/mod.rs` 收缩为组合根；窗口回调、窗口位置保存、显示/隐藏、重绘和添加源窗口同步迁移到 `src/ui_bridge/window_lifecycle.rs`。
- 配置层移除 `Hotkey::label()`、`slint_key_to_vk()` 和 `ProcessInfo::label()`；展示格式集中到 `src/presentation.rs`，Slint 输入映射回到 `ui_bridge/input.rs`。
- `UiCache` 不再保存设备筛选和默认设备配置镜像；设备属性由 Slint 投影/AppConfig 读取。
- 新增 `src/audio/errors.rs` 统一 capture/render 设备失效 HRESULT 分类；删除引擎无意义的 `copy_wave` 包装。
- 补充 Rust bridge、音频错误边界、窗口生命周期、源行模型和 `ui/components/source-row.slint` 的关键中文注释。
- 同步 `ARCHITECTURE.md`、`BACKEND.md`、`FRONTEND.md`、`PROJECT.md`。

## 八、已运行验证

- `scripts\\build-dev.cmd fmt -- --check`：通过。
- `scripts\\check.cmd`：通过；保留非阻断的行数审查警告：`src/ui_bridge/callbacks/route/sources.rs`、`ui/app-window.slint`、`ui/components/add-source-panel.slint`。
- `scripts\\build-dev.cmd test`：通过，38 项测试。
- `scripts\\smoke.cmd`：通过。
- 软件渲染快照：route、route 添加音频源、player 菜单均成功生成 RGBA 文件；临时文件已删除。

## 九、未覆盖与保守取舍

- Slint 外部源行属性契约未改变，只收窄了 Rust 内部接缝；避免把布局和 callback 回归混入本轮。
- 音频热路径没有在缺少真实设备 WAV/route 基线时强行改变分配和节拍逻辑；现有 ring、Converter 和线程模型保留，待实机逐采样/听感和 CPU/RAM 基线后再单独处理。
- `UiCache` 中音频库条目、分类和卡片级派生态仍保留，因为它们当前承担过滤、分页、槽位和交互生命周期，不是可即时读取的单一配置字段。
- 真实设备路由听感、`--capture` WAV 逐采样、`--route` 长时间运行、桌面鼠标操作及 RAM/CPU/启动时间仍未实测，不标记为通过。

下一步：在本阶段证据基础上继续拆分音频 capture/format 原语前，先建立真实设备基线；不以静态重命名替代音频行为验证。

## 十、全仓注释标准升级

本轮新增的注释要求已提升为长期工程标准，并写入根目录：

- `AGENTS.md` 第 3.2.1 节：所有 Rust/Slint 文件、私有成员、函数方法和关键内部机制都必须有中文注释。
- `FILE-CONVENTIONS.md` 第 2 节：明确字段、枚举变体、Slint property/callback/function、线程/所有权/FFI 前提和交互副作用的注释覆盖要求。

后续任何代码切片都必须先检查所在文件的注释覆盖；历史未覆盖代码必须列入阶段记录，不能以“本轮未修改”为理由宣称全仓完成。

## 十一、全仓注释收口实测

已覆盖 `src/**/*.rs` 与 `ui/**/*.slint` 的模块职责、结构/字段、枚举变体、函数/方法、Slint 组件/property/callback/function 及关键内部机制注释。注释标准已写入 `AGENTS.md` 与 `FILE-CONVENTIONS.md`，后续新增代码继续遵守。

本轮修复了注释代理引入的纯结构问题，但未改变运行逻辑：

- 恢复 `ui/theme.slint` 与 `ui/overlays/global-overlays.slint` 的合法 global 声明。
- 恢复解码器 `MF_INIT`、MF buffer bytes/Unlock 和缺失闭合括号。
- 删除 `audio/sfx.rs` 重复的 `StreamPlayer` 声明。
- 恢复 `engine/monitor.rs` 后台线程对 `SharedMonitor` 的 clone，并将不适用于 block 的 doc comment 改为普通注释。

验证：

- `cargo fmt -- --check`：通过（直接调用 cargo；批处理脚本传递 `-- --check` 时会触发 Windows 命令行长度错误）。
- `scripts\\build-dev.cmd build`：通过。
- `scripts\\build-dev.cmd check`：通过。
- `scripts\\build-dev.cmd test`：通过，38 项测试。
- `scripts\\smoke.cmd`：通过，生成设备/进程列表及路由、菜单、分类、三种音源窗口、点击和声纹快照。

未覆盖为行为测试的内容：真实设备听感、WAV 逐采样、桌面鼠标手工操作、CPU/RAM/启动时间；本轮只补注释，不把这些未实测项写成已验证。

## FRONTEND.md — Slint 界面边界与音频库契约

本文件记录当前 Slint 页面、桥接层与音频领域 API 的实际接缝。修改契约时，
同次更新 `ui/app-window.slint`、对应桥接回调和本文档。

## 1. 文件职责

`ui/app-window.slint`：窗口顶栏、全局菜单遮罩和常驻状态栏。
`ui/components/page-host.slint`：装配三页、持有页面间绑定并转发 bridge 回调。
`ui/components/app-header.slint`、`tab-bar.slint`：产品标识、实时引擎状态和三页导航。
`ui/components/status-bar.slint`：状态、输出友好名、混麦开关/麦选择、总音量滑块与启停。
`ui/overlays/context-menu.slint`：遮罩、分类/条目菜单、坐标钳制和动作转发。
`ui/overlays/diagnostic-panel.slint`、`ui/overlays/global-overlays.slint`：右侧抽屉式问题中心（问题列表 + 日志开关）、
`DiagState` 全局状态与顶层覆盖层宿主；状态栏与设置页直接读写该全局，不经 `app-window` 透传。
`ui/components/diagnostic-row.slint`、`ui/components/diagnostics-card.slint`：问题行/徽标与设置页诊断卡片。
`ui/pages/route.slint`、`ui/components/source-row.slint`：渲染实际选择的进程/文件/麦克风源；启停、静音、增益、每源输出和麦克风选择均接 bridge。
`ui/components/source-actions-menu.slint`：源卡片右侧更多菜单；提供删除源、单独转发、音频文件设置快捷键和定位音频库。
`ui/components/category-sidebar.slint`：真实 `/` 分类路径、拖拽排序、右键操作。
`ui/pages/settings.slint`：常规/音频设备/快捷键/VB-CABLE/关于导航；设备、默认角色、热键与安装操作走现有 bridge 回调。
`ui/components/*.slint`：只复用无领域权威控件；图标使用 Windows `Segoe MDL2 Assets`，不新增依赖或图片包。
`ui/theme.slint`：统一颜色、圆角、间距、控件尺寸与状态色。

Rust 桥接：

- `src/ui_bridge/mod.rs` 负责单实例、窗口装配、事件循环和退出 flush。
- `state.rs` 持有 `AudioEngine`、`AppConfig`、保存队列、热键、电平历史与异步进程刷新结果。
- `cache.rs` 持有索引、菜单/点击态、列表快照、暂停恢复点和播放目标。
- `callbacks/` 按路由、分类、播放和设置拆分回调。
- `src/library.rs` 提供分类路径、可见索引、递归变更和文件验证规则。
- 文件播放与试听通过 `AudioEngine` API；桥接不直接调用 decode、SFX 或 PreviewEngine。
- bridge 仍访问 `audio::device` / `audio::policy`、原生文件对话框和 VB-CABLE 安装。
- 刷新进程/设备按钮在 worker 枚举所有列表，50 ms 状态定时器把结果写回 Slint；设备分类开关走同一刷新路径。
  - Process、file、mic 各自的 render ID 保存到 `OutputConfig`；启动时 `SourceOutputRoutes` 将来源路由到各自 render 端点。
  - 进程行开关只切换当前进程的启用状态，关闭后保留卡片；删除动作才从路由源集合移除进程。
  - 文件行开关是暂停/继续，不清除引擎播放请求或当前位置；删除源只隐藏路由行，不删除音频库条目和磁盘文件。
  - 更多菜单的“单独转发”只打开选中的一个源，使用该源保存的输出设备，绕过底部总开关的多源选择。
  - 麦克风和进程源提供删除/单独转发；快捷键只对音频文件启用，避免伪造没有配置模型支撑的进程热键。
  - 音量列隐藏重复的百分比标签，源卡片右侧操作组固定横向排列，剩余宽度优先给音量滑轨。
- `SaveQueue` 以 300 ms 防抖串行写配置，退出时 flush。
- 诊断：`src/diagnostics/` 汇总 WARN/ERROR，`ui_bridge/diagnostics.rs` 每 50 ms 把快照灌入 `DiagState`；
  状态栏徽标显示问题数与未读数，设置页「诊断」分组与问题中心共用同一面板。
- 调试开关按 11 个组件分类控制日志文件级别，改动即时重载 EnvFilter 并写入 `AppConfig.debug`。

## 2. 展示状态

- Slint 不持有 PCM；只显示捕获、麦克风、render 三组各 32 点的峰值历史。
- 50 ms 状态轮询从 `AudioEngine` 读取引擎状态、电平和文件播放快照。
- 音量运行态在 `AudioEngine`；持久化值在 `AppConfig`。
- `UiCache` 是桥接视图缓存；其配置镜像字段尚未完全收口。
- 界面错误短句使用 `Error::user_message()`；HRESULT 和内部原因只进 tracing。

## 3. Slint 契约

- `MainWindow` 由 `ui/app-window.slint` 导出；页面绑定属性并上报回调。
- 回调注册位于 `src/ui_bridge/callbacks/`，由 `ui_bridge/mod.rs` 统一装配。
- 页面切换、Esc 和点击遮罩外区域都会关闭菜单；菜单组件自行钳制窗口坐标。
- 删除属性或回调前，检查 Slint 绑定、Rust 注册和实际触发点。
- 全局 `toggle_route` 使用 RegisterHotKey；另有 `previous_track`/`next_track`/`stop_playback`
  三条内置全局热键，以及每个音频条目可选的全局组合键（`AudioEntry.hotkey_binding`）。
  绑定按 id 分区：`1..=4` 内置动作，`1000 + 条目下标` 为条目；`WM_HOTKEY` 的 `wParam` 即 id。
- 冲突检测三层：自家 `(mods, vk)` 全等比较、外部占用（`RegisterHotKey` 失败经
  `take_failures()` 回报 UI）、无修饰键单键拒绝（会吞掉正常输入）。

- 音频条目旧字段 `hotkey`（前台单键，仅在窗口获焦时触发）仍保留；新绑定优先显示。

### 开发期 Live Preview

`dev.ps1` 默认使用 Slint 1.18 的官方 Live Preview：以 `--features slint/live-preview` 构建并设置编译环境变量 `SLINT_LIVE_PREVIEW=1`。Slint interpreter 在运行时监听 `.slint` 源文件，修改界面后保留进程、属性、模型与回调；修改 Slint 与 Rust 交互的属性/回调签名时仍须重编并重启。

Live Preview 是开发专用功能，不写入 `Cargo.toml` 默认 features。正式 `scripts\build-release.cmd` 不启用此 feature。`dev.ps1` 不监视 `.slint` 文件做 Cargo 重建；仅 Rust/build/Cargo 文件改变时重新构建和重启。

`SLINT_LIVE_PREVIEW=1` 必须在构建时设置，工作目录保持仓库根目录以便运行进程读取 `ui/` 下的源文件。


## 4. 音频库语义

- 分类以 `/` 表示父子路径；“全部”和“未分类”默认存在并记录在 `AppConfig.audio_categories`，用户可删除。
- 删除“全部”清空软件内全部音频条目；删除“未分类”移除该分类及其后代条目；磁盘文件不删除。删除普通分类则把相关条目移到“未分类”。
- 空白区域或删除全部分类后的树区右键新建顶层分类；右键已有普通分类新建其子分类。默认名称为“未命名分类”，同级重名自动使用“未命名分类 2”等后缀。
- `src/library.rs` 负责分类路径、递归匹配、重命名/移动/删除；桥接按分类、搜索词、名称排序和每页 20 条生成文件表。
- 可见行下标按相同过滤/排序/分页序列映射回配置条目；删除、播放、循环、快捷键都使用该映射。
- `AudioEntry` 保留时长、字节数、采样率、声道、循环和可选单键快捷键；导入/迁移读取文件元数据，码率显示为按文件大小/时长推算值。
- 单击选择、450 ms 内双击播放；播放状态/进度来自 `AudioEngine::file_state()`。
- 详情“播放”进入路由目标；“本机试听”进入 monitor 目标，二者分别保持现有路由/监听语义。
- 详情主音量滑块只写 `preview_volume` 并调用 `AudioEngine::set_monitor_volume()`；不改路由 SFX 增益。
- 路由播放的本机监听开关仍写 `preview_enabled`；暂停时底部进度/切换控制显示，空闲时隐藏。
- 拖动进度通过目标位置重建播放流；快捷键在本程序获焦时触发，重复分配和与路由主键冲突会拒绝。

## 5. 当前边界

- 路由页源卡片的标题、输出设备、音量和操作控件采用紧凑横向布局；卡片数量过多时仍由内容区 `ScrollView` 纵向滚动。
- 路由页进程切换关闭时保留源行；音频文件切换关闭/打开保留暂停位置。
- 源右侧更多菜单不跳设置页；“单独转发”只创建选中源的输出链路。
- 进程行名称列展示进程名/PID，启停由全局 route lifecycle 控制；开始后不能在同一会话动态变更进程槽。
- 文件行暂停/停止共享 AudioEngine 播放状态；文件循环、seek、试听均复用已有播放控制。
- 进程/文件/麦克风输出 ID 分别持久化，启动路由时 `SourceOutputRoutes` 将各输入送到各自 render 端点。
- 详情页本机监听音量只作用于 `AudioEngine` monitor；文件路由增益继续由路由页文件音源行控制。
- 音频快捷键字段由配置持久化，只在 ProcessMic 获得键盘焦点时触发；不是全局游戏热键。
- 设置侧栏可切换常规、音频设备、快捷键、虚拟线缆和关于分组；只显示现有后端能力。
- 音频表按名称过滤/排序，按每页 20 条分页；实际格式、采样率、估算码率、文件大小、路径来自本地音频条目。
- 音频列表采用 `ScrollView`；超大页表使用分页，不加载全库行到同一页。
- `--snapshot-ui --category-menu` 与 `--snapshot-ui --category-row-menu` 模拟分类右键。

## 6. 验证

```bat
scripts\check.cmd
scripts\smoke.cmd
target\debug\mintcat-proc-mic.exe --snapshot-ui --page route --out route.raw
target\debug\mintcat-proc-mic.exe --snapshot-ui --page settings --out settings.raw
target\debug\mintcat-proc-mic.exe --snapshot-ui --out player.raw
target\debug\mintcat-proc-mic.exe --snapshot-ui --menu --out player-menu.raw
```

快照为原始 RGBA，文件头 8 字节是宽、高小端 u32；临时产物不要提交。
音频完整路径仍需手工验证：路由有声、音量有效、切歌/停止/拖动进度和右键动作。

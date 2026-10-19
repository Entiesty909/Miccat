# 音频库页面 1:1 重构计划

状态：待实施；本轮只完成现状截图与实施计划，不改业务代码。

参考：用户提供的音频库设计稿（1536×1024）。
当前快照：本轮已用 `--snapshot-ui --page player` 生成并展示当前页面截图（1280×800）。

## 1. 目标与边界

### 目标

这不是静态 UI 仿制。页面显示、数据模型和用户操作必须形成完整闭环：

```text
真实 AppConfig / 文件系统
        ↓
音频库领域规则与 AudioEngine 播放
        ↓
ui_bridge 派生列表/详情/状态
        ↓
Slint 三栏页面与回调
        ↓
分类、筛选、播放、试听、音量、循环、快捷键、删除/移动、配置保存
```

把用户提供的设计稿作为 UI 和行为契约：列表数据显示真实条目，详情控件修改真实配置或引擎状态，刷新/排序/分页后的行操作仍映射回正确的 `AudioEntry`。

### 不变的产品边界

- Windows 原生桌面、Rust 2024、Slint、WASAPI、Media Foundation 和现有依赖集合不变。
- 不新增数据库、网络服务、登录、插件或第二份音频引擎。
- 低层 WASAPI/解码实现只在现有接口无法表达设计稿行为时调整；页面逻辑优先收口到 `library`、`AudioEngine`、`AppConfig` 和 `ui_bridge`。

### 明确不做

- 不把设计稿中的示例音频名、文件路径、分类数量、快捷键、输出设备名写入任何源码、默认配置或测试数据。
- 不把音频 PCM 放进 Slint；波形只允许使用视图层的有限峰值数组。
- 不把“添加文件夹”、播放列表、收藏、随机播放、视频预览、宏按键等设计稿之外的功能混入本轮。
- 不改变路由页、设置页和全局顶栏语义；本轮只改音频库 tab 内容区。

## 2. 当前实现与设计稿差异

依据当前源文件和本轮真实快照，主要差异如下：

| 区域 | 当前实现 | 设计稿目标 |
|---|---|---|
| 窗口顶部 | 本轮不改全局顶栏和页签 | 只核对音频库 tab 内容区 |
| 页面骨架 | `PlayerPage` 用分类栏 + 中央 `Card` + 340px 详情卡 | 左分类栏、中央列表、右详情三栏，边界和留白更清晰 |
| 工具栏 | 搜索、添加音频、名称排序同一行 | 搜索框、添加按钮、更多按钮；下一行格式筛选和排序 |
| 列表表头 | 名称、分类、时长、快捷键、状态；没有独立格式列和选择框 | 选择框、名称、分类、时长、格式、快捷键、状态、行操作 |
| 行视觉 | 通用 MDL2 音频图标，58px 行高，元数据合并为一列 | 条目图标、名称、格式/码率信息、彩色分类标签、独立状态和播放/更多操作 |
| 详情区 | 详情、输出下拉、音量、快捷键、循环、分类、本机监听、文件信息 | 顶部封面/图标和标签、播放/试听/更多操作、波形和进度、音频设置、底部移动/删除操作 |
| 播放控制 | 独立 `AudioTransport` 卡片，含上一首/暂停/停止/下一首 | 详情区显示当前音频控制，底部保留紧凑的全局状态栏；播放状态不能重复造权威 |
| 数据稳定性 | 页面从本机配置和设备读取真实数据 | 不加入案例数据源，空库按真实空态显示 |
| 结构风险 | `AudioList`、`AudioDetail`、`PlayerPage` 已有职责，但缺少格式筛选、详情波形和设计稿所需视图模型字段 | 继续按组件拆分，Slint 只负责显示和事件上报 |

当前桥接已具备可复用行为：分类/搜索/排序/分页、单击选择、双击播放、停止、进度、输出设备、试听、音量、循环、快捷键绑定、右键菜单和配置防抖保存。重构优先改视图契约，不重写这些领域行为。

## 3. 数据容器与真实逻辑方案

### 3.1 只使用真实数据容器

设计稿中的音频名、快捷键、分类数量、设备名和状态只是视觉案例，不进入仓库任何代码、配置默认值或测试数据。

页面统一使用现有真实数据容器：

- `AppConfig.audio_entries`：音频条目、分类、元数据、循环和快捷键的持久化权威。
- `AudioEngine`：播放、试听、暂停、停止、进度和运行时音量的权威。
- `UiCache`：当前分类、搜索词、格式筛选、排序、分页、选中下标和菜单坐标等派生态。
- Slint `ModelRc`：只承载桥接层投影出的名称、格式、时长、状态和详情字段，不保存 PCM。

快照也只读取当前真实配置和当前机器设备；不注入示例条目，不创建虚假路径，不注册案例快捷键。需要视觉基线时，使用用户机器现有音频库或由调用者在仓库外准备临时配置，临时数据不提交。

### 3.2 显示模型字段

桥接层从真实条目计算并灌入页面容器：

- `categories`：分类名称、层级和真实计数。
- `entries`：显示名、分类、时长、格式、元数据、快捷键标签、状态和图标。
- `selected_entry`：当前可见行及其稳定配置下标映射。
- `detail`：真实输出设备、音量、循环、本机试听、文件大小和路径。
- `waveform`：最多 32 个视图峰值；不读取或保存完整 PCM。
- `transport`：引擎播放快照投影出的当前位置、总时长、暂停和停止状态。

所有字段都由真实配置/引擎计算；页面不能用设计稿文字补齐缺失值。

### 3.3 快照入口

保留现有快照入口：

```text
--snapshot-ui --page player --out <临时文件>
```

快照不改变生产配置，也不拥有一套独立的演示数据源。设计稿只作为截图核对参照，不作为运行时输入。

## 4. 实施切片

### Slice A：真实数据基线

**文件范围**

- `src/ui_bridge/library.rs`
- `src/ui_bridge/cache.rs`
- `src/ui_bridge/callbacks/library_view.rs`
- `ui/pages/player.slint`
- `ui/components/audio-list.slint`
- `ui/components/audio-detail.slint`

**工作**

- 先从真实 `AppConfig.audio_entries` 建立格式、分类、搜索、排序和分页的统一可见索引。
- 所有列表行和详情操作继续通过稳定条目下标映射，不使用设计稿样例替代真实条目。
- 先建立真实数据下的视觉快照基线，再进行布局切片。

**验收**

- 页面没有仓库内置的示例音频、快捷键或设备名称。
- 默认快照仍读取真实配置；`AppConfig::default()` 保持空音频库。
- 当前筛选、分页和选中条目在刷新后映射一致。

### Slice B：音频库 tab 页面骨架

**文件范围**

- `ui/pages/player.slint`
- `ui/components/category-sidebar.slint`
- `ui/components/audio-list.slint`
- `ui/components/audio-detail.slint`
- `ui/components/audio-transport.slint`

**工作**

- 只重做音频库 tab 的内容区；不修改全局品牌顶栏、页签导航或其他页面外壳。
- 按设计稿固定分类栏、列表栏、详情栏的伸缩关系、边界、留白和滚动容器。
- 统一卡片、表头、行高、按钮、标签、状态点和详情区控件的视觉层级。
- 所有动态文本、分类、音频名、快捷键、设备名和状态都来自真实桥接模型，不在 Slint 写案例数据。

**验收**

- 只切换到音频库 tab 即可核对设计稿对应区域；路由、设置和全局顶栏保持原样。
- 真实音频条目过多时列表和详情仍可滚动，窗口缩放不破坏三栏关系。
- 没有任何设计稿示例字符串进入组件源码。

### Slice C：音频库三栏骨架与工具栏

**文件范围**

- `ui/pages/player.slint`
- `ui/components/category-sidebar.slint`
- `ui/components/audio-list.slint`
- `ui/components/audio-detail.slint`
- `ui/components/audio-transport.slint`
- 视重复情况新增：
  - `ui/components/audio-library-toolbar.slint`
  - `ui/components/audio-format-tabs.slint`
  - `ui/components/audio-waveform.slint`

**工作**

- 按设计稿固定左栏、列表栏、详情栏的最小宽度和伸缩关系。
- 工具栏改为搜索、添加音频、更多按钮；格式筛选与名称排序单独成行。
- 分类栏保留真实分类树、计数、拖拽和右键菜单，不把分类样例写入组件。
- 列表加入设计稿所需的表头、格式列、状态点、选择态和行操作视觉层。
- 详情区按设计稿重排：封面/图标、名称/元信息/标签、播放组、波形/进度、音频设置和底部操作。
- 旧 `AudioTransport` 只保留实际需要的播放控制；不与引擎重复保存播放状态。
- 所有动态文本继续使用 Slint 正确插值语法 `\\{...}`。

**验收**

- 使用真实配置快照核对三栏、工具栏、列表和详情与参考图逐项对齐。
- 长名称只在自己的列/视口内省略，不改变其他列宽。
- 列表超出可视区域仍可滚动；分页与现有每页 20 条语义不被破坏。

### Slice D：桥接视图模型与真实数据接线

**文件范围**

- `src/library.rs`、`src/library/tests.rs`
- `src/config/mod.rs`、`src/config/store.rs`
- `src/engine/playback.rs`、必要时 `src/engine.rs`
- `src/ui_bridge/library.rs`
- `src/ui_bridge/cache.rs`
- `src/ui_bridge/callbacks/library.rs`
- `src/ui_bridge/callbacks/library_entries.rs`
- `src/ui_bridge/callbacks/library_view.rs`
- `src/ui_bridge/callbacks/player.rs`
- `src/ui_bridge/status.rs`
- `ui/components/page-host.slint`
- `ui/app-window.slint`（仅同步新增属性/回调契约）

**工作**

- 列表格式、显示状态、图标、详情摘要、输出设备和波形改为桥接层模型输出；Slint 不从路径字符串猜业务字段。
- 分类树、分类计数、格式筛选、名称搜索、排序和分页使用同一套可见索引；筛选/排序后所有播放、删除、移动、快捷键操作仍映射回稳定的 `AudioEntry`。
- “添加音频”真实打开文件选择器、验证文件元数据、写入当前分类并经 `SaveQueue` 防抖保存；无效文件只进入现有问题中心短句。
- “播放”按详情里的输出目标送入路由；“本机试听”只打开 monitor；输出设备、音量、循环和分类修改分别更新引擎或配置的唯一权威。
- 详情音量必须明确作用范围：若沿用当前全局 SFX 增益，页面文案和状态只显示全局值；若设计稿要求条目独立音量，则给 `AudioEntry` 增加迁移兼容的持久化字段，并让播放请求携带该增益，禁止只改 UI 数字。
- 快捷键显示和绑定继续使用真实 `RegisterHotKey`/配置冲突检测，不注册设计稿案例快捷键。
- 真实数据和所有页面模型共用同一填充路径，禁止为截图复制一套生产 UI。
- `UiCache` 只保留搜索词、格式筛选、分页、排序、选中下标和菜单坐标等派生态；音频条目仍以 `AppConfig` 为持久化权威，播放态仍以 `AudioEngine` 为权威。

**验收**

- 使用真实配置时新增/删除/移动/重启后数据仍一致。
- 搜索、分类、格式筛选、排序、分页、选中和详情同步正确。
- 播放/暂停/停止/进度拖动/循环、路由输出和本机试听都能从详情控件完成且状态可回写。
- 快捷键绑定、冲突提示和播放触发真实生效，不只是列表显示。
- 配置写入仍走 300ms 防抖队列，滑块拖动不逐事件写盘。

### Slice E：交互回归与文档同步

**文件范围**

- `FRONTEND.md`
- `ARCHITECTURE.md`
- `DEBUGGING.md`
- 新增递增编号的 `docs/step-XX-audio-library-redesign.md`
- 受影响的纯逻辑测试文件

**工作**

- 更新 UI 契约、真实数据快照入口、三栏布局和状态权威说明。
- 对格式筛选、显示索引映射、时长/位置格式化等纯逻辑补边界测试；不写真实设备测试。
- 删除重构后无调用点的旧布局属性、死回调和重复显示字段，不留兼容壳。
- 记录真实构建、快照、交互和未覆盖的真实设备限制。

## 5. 视图契约草案

页面最终需要的新增派生字段以现有数组模型为基础，集中在桥接层输出：

```text
entry-icons
entry-formats
entry-metadata
entry-categories
entry-durations
entry-hotkeys
entry-statuses
entry-selected
format-filter
selected-detail-waveform  // 最多 32 点，不是 PCM
```

事件仍按领域动作表达：

```text
format-filter-changed
entry-selected
entry-double-clicked
entry-play
entry-preview
entry-progress-seek
entry-output-changed
entry-volume-edited
entry-loop-changed
entry-hotkey-bind
entry-context
```

不新增一个“设计数据状态”到 `AudioEngine`；页面只把真实配置和引擎状态转成显示字段。

## 6. 逐项验收清单

### 视觉

- [ ] 1536×1024 下品牌顶栏、页签、内容区和底部状态栏高度与参考图一致。
- [ ] 左分类栏、中央列表、右详情栏比例、边框、圆角和留白一致。
- [ ] 搜索、添加音频、更多、格式筛选、排序控件位置一致。
- [ ] 列表表头、选择框、图标、分类标签、时长、格式、快捷键、状态和行操作位置一致。
- [ ] 详情头部、播放按钮、试听按钮、波形、输出、音量、快捷键、循环、分类和底部操作位置一致。
- [ ] 长文本省略、列表滚动、详情滚动和窗口缩放不破坏布局。

### 行为

- [ ] 真实音频条目和真实快捷键来自 `AppConfig`，没有案例数据污染配置。
- [ ] 单击只选择；双击/播放按钮按现有语义播放；停止、暂停、进度拖动和循环保持有效。
- [ ] 本机试听只走 `AudioEngine` monitor，不把试听设备当路由播放终点。
- [ ] 删除、移动分类、右键菜单、快捷键冲突检测和配置防抖保存保持有效。
- [ ] 无音频、无选中项、空分类、进程退出、设备失效和初始化失败仍显示现有用户短句。

### 工程门禁

```bat
scripts\\check.cmd
scripts\\smoke.cmd
scripts\\build-dev.cmd test
```

设计快照：

```bat
target\\debug\\miccat.exe --snapshot-ui --page player \\
  --out "%TEMP%\\miccat-audio-library.raw"
```

临时 `.raw`、`.png` 和对比图放在 `%TEMP%`，不进入仓库。UI 行为还需启动真实窗口手工检查；真实路由声音继续使用已有 `--probe-playback` 和设备验收路径。

## 7. 实施顺序与停止条件

严格按 Slice A → B → C → D → E 顺序，每个切片完成后先构建再截图，不在同一切片顺手改路由、引擎或设置页。任一切片出现以下情况必须停在该切片修正：

- 任何设计稿示例数据进入 `AppConfig::default()` 或生产启动路径。
- Slint 组件开始持有音频状态、PCM 或解码线程。
- 页面回调出现文件 IO、设备枚举、同步解码或线程 join。
- 真实音频库行为为了“看起来一致”而被伪造或覆盖。

完成标准：设计稿视觉核对通过、真实配置行为回归通过、三项门禁通过，并写入对应阶段实测文档。

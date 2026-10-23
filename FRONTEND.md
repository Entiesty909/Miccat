## FRONTEND.md — Slint 界面边界与音频库契约

本文件记录当前 Slint 页面、桥接层与音频领域 API 的实际接缝。修改契约时，
同次更新 `ui/app-window.slint`、对应桥接回调和本文档。

## 1. 文件职责

`ui/app-window.slint`：窗口骨架与焦点作用域；纵向排列全宽顶栏、中央内容行、全宽底栏，并承载全局菜单遮罩。
`ui/components/app-header.slint`：58px 品牌操作栏；图标、Miccat 字样和版本靠左，路由页的“添加音频源”与全局操作靠右并垂直居中。添加入口复用原独立窗口回调；路由页不再重复显示添加和“刷新进程”按钮。
`ui/components/sidebar-nav.slint`：45px 仅图标路由、音频库、设置侧栏；只占中央页面行高度，点击切换主页面，无 Tooltip。
`ui/components/status-bar.slint`：36px 底栏左侧仅显示转发状态，右侧排列音源入口、总音量滑块和诊断问题数；原错误短句/详情按钮不再占底栏，详情统一从问题中心进入。
- `ui/themes/light.slint`、`dark.slint`、`pink.slint`：分别保存三套颜色 token；`ui/theme.slint` 只做主题索引调度。标准 `CheckBox`/`LineEdit` 由 `Palette.color-scheme` 同步黑白主题，所有自定义窗口/右键菜单/跑马灯弹层读取 Theme token。
主题预设弹层仅显示 2×2 风格色块，不显示文字标签；主题选择保存在 `AppConfig.theme_index`，下次启动时首帧恢复。粉色窗口底色不透明，原图作为窗口内背景绘制；页面 72% 浅粉遮罩在后台按当前窗口逻辑尺寸预合成到一张不透明壁纸，顶栏/侧栏/底栏仍保留原图；下载中或拖动窗口到新尺寸时暂用原图与透明遮罩，180 ms 尺寸防抖后切回预合成图。页面内容卡片仍为 60% 不透明表面，字段为 92% 不透明；弹层仍使用 90% 不透明的 `Theme.card`。
`ui/overlays/context-menu.slint`：遮罩、分类/条目菜单、坐标钳制和动作转发。
`ui/overlays/diagnostic-panel.slint`、`ui/overlays/global-overlays.slint`：右侧问题中心抽屉分为标题、问题/日志页签、可滚动列表、当前详情和操作区，日志分类独立滚动；`DiagState` 维持唯一数据和操作出口，状态栏徽标及设置页入口复用同一面板。
`ui/components/diagnostic-row.slint`、`ui/components/diagnostics-card.slint`：问题行/徽标与设置页诊断卡片。
`ui/pages/route.slint`、`ui/components/source-row.slint`：进程、文件、麦克风卡片共用信息、输出和转发音量区域；文件卡片额外展示播放控制，启用转发开关移到设置区第三行，不再挤压转发音量百分比；进程/麦克风的开关保留右侧原位置。文件卡片窄窗口时收紧信息、播放和设备控件宽度。进程两行显示进程名与 `PID：数字`，文件只显示音频库备注名，麦克风两行显示固定标题及设备型号；文件分类在悬停时查看。进程预览音量仍为禁用占位，已启用卡片左侧显示蓝色竖线。进程和麦克风点击转发音量行左侧的原音量图标可静音/恢复。右上角上三角在 150 ms 翻转后打开源菜单，转发中且已启用的卡片禁用此入口。
文件卡片未启用转发或路由未运行时仍可播放/暂停、前后切歌和切换四种播放模式；本机默认播放端负责试听，只有运行中已启用的文件槽才另送路由输出。播放中启用转发或启动路由，从当前媒体位置接入输出，不从头重播。播放或暂停时可拖动进度，松开后在本机试听和已启用的路由槽从指定 100 ns 位置重建，未播放时拖动不触发播放；按钮说明使用 Tooltip。
路由卡片输出下拉按 Windows 友好名称逐项勾选目标，进程/文件按槽位、麦克风按当前源分别保存可见端点 ID；配对扬声器与虚拟麦克风的勾选状态独立，运行时才映射到 render ID。设备切换后不以列表下标误选，路由启动只收集启用卡片的目标。运行中目标选择锁定，停止后可重新配置。播放页设备选择只作为新文件源的默认值，不覆盖既有文件卡片。
- `ui/components/add-source-panel.slint`：按真实进程、音频库和物理麦模型添加路由源；音频/麦克风双栏各自成卡、不包额外底色容器；麦克风列表从首个真实设备起纵向排列，添加麦克风成功后关闭独立窗口；路由麦克风源行第一行固定显示“麦克风”，第二行灰色小字显示所选物理设备的 Windows 友好名称。普通“添加音频源”入口每次重置为第一个“进程”Tab；替换动作仍显式打开对应源类型。音频/麦克风设置区按设备多选，至少保留一个目标，添加后新卡片显示同一组选择。
`ui/components/audio-source-list.slint`、`audio-source-details.slint`：分别显示可搜索的真实音频条目列表和所选条目的元数据/路由音量设置。
`ui/components/source-actions-menu.slint`：源卡片右上角上三角菜单；由独立 `PopupWindow` 承载，按源类型提供替换、删除、单独转发，文件源另含定位、快捷键和停止播放。
`ui/components/category-sidebar.slint`：真实 `/` 分类路径、计数、拖拽排序、右键编辑和分类图标，图标与名称使用同一横向布局；底部不再有快捷键/管理区块。音频库顶栏 `audio-transport.slint` 是贴合页面上缘的 44px 满宽播放条，播放控制为无底色图标。
`ui/components/audio-entry-editor.slint`：分类/条目共用的名称与音符/首字/自定义图标编辑内容；由独立、非模态的 `AudioEditWindow` 承载，标题随编辑对象显示在 Windows 标题栏中，条目名称只改软件显示。
`ui/pages/settings.slint`：常规/音频设备/快捷键/VB-CABLE/关于导航；设备、默认角色、热键与安装操作走现有 bridge 回调。
`ui/components/*.slint`：只复用无领域权威控件；图标使用 Windows `Segoe MDL2 Assets` 或 `resources/icon.ico`，不新增依赖或图片包。

`ui/components/marquee-text.slint`：按 Slint `Text.preferred-width` 测量单行文本，裁剪视口；只在溢出文字悬停时启动计时滚动，离开复位。
`ui/components/marquee-combo-box.slint`：复用字符串模型与 `current-index` 的下拉框；选项列表由 `PopupWindow` / 虚拟化 `ListView` 承载。
`ui/theme.slint`：统一颜色、圆角、间距、控件尺寸与状态色。

Rust 桥接：

- `src/ui_bridge/mod.rs` 负责单实例、初始化、窗口装配、事件循环和退出 flush。
- `src/ui_bridge/window_lifecycle.rs` 负责独立窗口回调、位置持久化、显示/隐藏、重绘和添加源窗口属性同步。
- `state.rs` 持有 `AudioEngine`、`AppConfig`、保存队列、热键、电平历史、异步进程刷新结果和独立窗口集合 `AppWindows`。
  `AppConfig.windows` 是主窗、添加源窗和编辑窗位置的持久化权威；窗口关闭或从音频加入路由后隐藏时保存坐标。
- `cache.rs` 只持有列表、槽位和交互派生状态；设备筛选及默认设备配置从 Slint 属性/AppConfig 读取。
- `callbacks/` 按路由、分类、播放和设置拆分回调。
- `src/ui_bridge/sources.rs` 用统一 `SourceRows::push_*` 构造进程、文件、麦克风行；快照与运行时共享同一模型不变量检查。
- `src/library.rs` 提供分类路径、可见索引、递归变更和文件验证规则。
- 文件播放与试听通过 `AudioEngine` API；桥接不直接调用 decode、SFX 或 PreviewEngine。
- `ui_bridge` 仍访问 `audio::device` / `audio::policy`、原生文件选择器和 VB-CABLE 安装；`src/platform/window.rs` 在独立窗口显示后将其带到前台，不设置 owner，不禁用主窗口。
- 设置页刷新入口在 worker 枚举所有列表，50 ms 状态定时器把结果写回 Slint；设备分类开关走同一刷新路径。
  - Process、file、mic 各自的 render ID 保存到 `OutputConfig`；进程和文件卡片额外按槽位保存独立 render ID，启动时 `SourceOutputRoutes` 将每个输入送到自己的 render 端点。
  - 启动路由只把已启用进程/文件/麦克风的输出设备交给引擎冲突校验；关闭的卡片保留自己的设备配置，但不因配对端点阻止启动。
  - 进程行开关只切换当前进程的启用状态，关闭后保留卡片；删除动作才从路由源集合移除进程。
  - 文件卡片的播放、前后切歌和循环不依赖转发启用态；音频库单例监听槽负责本机试听，运行中且已启用的卡片另用对应 SFX 槽路由。关闭转发仅停止路由槽，试听继续；重新启用时若仍在播放，则从当前试听位置接入路由，停止态不自动播放。
  - 启动全局或单独转发不会自行播放文件；若卡片已在本机播放，输出图重建后按原位置恢复试听，并在该文件槽启用时同步路由。暂停/停止同时作用于当前试听与该卡片已运行的路由槽，不影响其它文件卡片。
  - 音频库顶部常显 44px 播放条，空闲时禁用操作；分类和详情各有一根 6px 拖动线及方向随开合变化的折叠按钮，折叠保留用户此前宽度。中间列表只显示垂直居中的图标/名称与时长，右键菜单按分页可见索引还原真实条目后加入路由主页。
  - 音频卡片前后切歌只在当前条目的精确分类内选择；手动前后切换首尾环绕。自然结束时，顺序播放在分类末尾停止，列表循环回到第一首，单曲循环重播当前条目，随机播放优先避开当前条目。
  - 播放模式按文件卡片槽位持久化到 `RouteConfig.audio_source_play_modes`；切歌后卡片条目路径同步写回配置。
  - 更多菜单的“单独转发”只打开选中的一个源，使用该源保存的输出设备，绕过底部总开关的多源选择。
-  麦克风和进程源提供删除/单独转发；快捷键只对音频文件启用，避免伪造没有配置模型支撑的进程热键。
- 麦克风卡片在设备列表已有真实麦克风但当前选择仍为“不混入麦克风”时，点击启用会先选择首个真实设备，再通过既有 `mic-mix-changed` 保存并更新卡片；运行中开关仍保持原有锁定规则。
-  添加源麦克风的“添加麦克风”按钮走独立提交回调：先同步选择到主路由、再隐藏添加源窗口；列表内切换设备仍只修改选择，不触发窗口关闭。
-  添加/替换进程前检查 PID 是否已在其它卡片；添加/替换音频前检查音频库实际条目下标，重复项拒绝提交并保留当前替换目标。
-  添加源麦克风的输出目标保存到 `AppConfig.output.mic_output_id`；默认“跟随全局默认设备”时清除单独目标。
-  输出设备下拉框的容器宽度固定；选中名称过长时只移动文字，使用往返横向滚动，不改变下拉框和列宽。
- 源操作状态由路由页统一维护，同一时间只显示一个源菜单；上三角在翻转过渡完成后才请求 `PopupWindow`，菜单关闭或页面切换后恢复入口；转发中启用的卡片禁止打开操作菜单。
- `resources/icon.ico` 同时用于主窗口、添加源窗口、音频库条目/分类编辑窗口，并作为 Windows exe 的数字资源 ID 1 嵌入，以显示在标题栏与任务栏；不显示在页签栏或路由卡片。
- `SaveQueue` 以 300 ms 防抖串行写配置，退出时 flush。
- 诊断：`src/diagnostics/` 汇总 WARN/ERROR，`ui_bridge/diagnostics.rs` 每 50 ms 把快照灌入 `DiagState`；
  状态栏徽标显示问题数与未读数，设置页「诊断」分组与问题中心共用同一面板。
- 调试开关按 11 个组件分类控制日志文件级别，改动即时重载 EnvFilter 并写入 `AppConfig.debug`。

## 2. 展示状态

- Slint 不持有 PCM；只显示进程、文件、麦克风和总线各 32 点的峰值历史。
- 50 ms 状态轮询从 `AudioEngine` 读取引擎状态、电平和文件播放快照；峰值由音频线程写入无锁 `AtomicU32` 窗口采样，桥接只更新变化的源行单元。
- 音量运行态在 `AudioEngine`；持久化值在 `AppConfig`。
- `UiCache` 是桥接视图缓存；`route_audio_entry` 只记录真实音频条目下标，不保存音频数据。
- 状态栏 Popover 显示每个源和总线的 32 点声纹/峰值 dB；总音量滑块旁显示总线增益 dB，Popover 的位置相对音源数触发区计算，不依赖固定窗口坐标。
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


### 路由跑马灯下拉

- 路由源卡片的输出设备、物理麦克风下拉，以及“添加音频源”对话框的进程下拉使用 `MarqueeComboBox`；对话框中的麦克风下拉和音频库/设置页的其他下拉维持标准 `ComboBox`。
- 菜单选项复用 `MarqueeText`，用实际 `preferred-width` 判断溢出；只激活鼠标所在选项的计时器。移开鼠标回到起点，文字末端短暂停留后往返循环。
- `PopupWindow.is-open` 控制选项列表子树；关闭菜单后列表行与其计时器立即销毁。`ListView` 只实例化可见选项。
- `MarqueeComboBox.marquee-enabled` 是可绑定的跑马灯总开关，默认 `true`；设为 `false` 时已选值和展开选项均停止跑马灯并以省略方式显示。
- 蓝色左侧指示条只绑定 `current-index` 所指向的实际已选项；鼠标悬停只改变悬停底色，不会移动选中指示条。
- `marquee-enabled=false` 时，闭合值和选项使用普通 `Text`，不创建 `MarqueeText` Timer；闭合文字视口固定约 `82%`，为右侧箭头保留安全区域。
- 当前索引仍 two-way 绑定到原属性；进程选择仍由 `process-index` 映射到缓存 PID，再触发原 `process-add` 回调。

### 下拉框视觉决策

本轮选择“保留标准 `ComboBox`，让 `MarqueeComboBox` 复刻其视觉结构”，不把所有下拉框强行替换为新控件。这样保留标准控件的成熟行为，同时避免同页出现明显割裂：同样的白色字段底、细边框、圆角、下拉箭头、悬停/按下底色和弹出列表内边距。已选项在重开列表时使用标准控件同类的浅灰选中底色；这属于样式统一，不另加独立的特殊交互。

### 开发期 Live Preview

`dev.ps1` 默认使用 Slint 1.18 的官方 Live Preview：以 `--features slint/live-preview` 构建并设置编译环境变量 `SLINT_LIVE_PREVIEW=1`。Slint interpreter 在运行时监听 `.slint` 源文件，修改界面后保留进程、属性、模型与回调；修改 Slint 与 Rust 交互的属性/回调签名时仍须重编并重启。

Live Preview 是开发专用功能，不写入 `Cargo.toml` 默认 features。正式 `scripts\build-release.cmd` 不启用此 feature。`dev.ps1` 不监视 `.slint` 文件做 Cargo 重建；仅 Rust/build/Cargo 文件改变时重新构建和重启。

`SLINT_LIVE_PREVIEW=1` 必须在构建时设置，工作目录保持仓库根目录以便运行进程读取 `ui/` 下的源文件。


## 4. 音频库语义

- 分类以 `/` 表示父子路径；“全部”和“未分类”默认存在并记录在 `AppConfig.audio_categories`，用户可删除。
- 删除“全部”清空软件内全部音频条目；删除“未分类”移除该分类及其后代条目；磁盘文件不删除。删除普通分类则把相关条目移到“未分类”。
- 空白区域或删除全部分类后的树区右键新建顶层分类；右键已有普通分类新建其子分类。默认名称为“未命名分类”，同级重名自动使用“未命名分类 2”等后缀。
- `src/library.rs` 负责分类路径、递归匹配、重命名/移动/删除；桥接按分类、名称搜索、名称排序和每页 20 条生成文件表。格式、采样率和码率作为真实元数据展示，不作为独立设计标签筛选。
- 中间列表只显示当前选中分类及其后代分类的条目；“全部”才显示全库。
- 可见行下标按相同过滤/排序/分页序列映射回配置条目；删除、播放、循环、快捷键都使用该映射。
- `AudioEntry` 保留时长、字节数、采样率、声道、循环和可选单键快捷键；导入/迁移读取文件元数据，码率显示为按文件大小/时长推算值。
- 单击选择、450 ms 内双击播放；播放状态/进度来自 `AudioEngine::file_state()`。
- 音频详情只展示文件元数据、分类和快捷键管理；不再提供输出设备或文件音量控件，避免把音频库选择写入路由输出配置。
- 路由页负责音频源的输出设备、路由增益、启停和单独转发；音频库的播放入口使用路由当前目标，不再拥有独立文件输出选择。
- 详情“本机试听”与音频条目播放行为仍复用现有引擎；输出设备配置只由路由卡片管理。
- 拖动进度通过目标位置重建播放流；快捷键在本程序获焦时触发，重复分配和与路由主键冲突会拒绝。

## 5. 当前边界

- 路由源卡片启用态只显示左侧蓝色竖线；卡片外框维持中性细边，不再使用整圈蓝色边框。
- 路由页进程源关闭时保留源行；文件源关闭转发只撤掉对应路由槽，本机试听不停止。
- 源右侧更多菜单由独立 PopupWindow 承载，自动避开卡片/滚动容器裁剪；点外部关闭，菜单内点击执行操作后关闭。
  - 文件源启用、路由运行和本机播放是三个独立状态；不开转发也能播放、切歌、循环；启用和开始转发不触发静止文件播放，播放中的文件按当前进度接入路由。
  - 文件卡片使用单例本机试听槽和独立的路由 SFX 槽；音频库另起播放会替换单例试听，不把库列表选择误认为卡片启用态。
  - Process、文件、麦克风输出 ID 分别持久化；物理麦克风是捕获源而不是可写 render 端点。虚拟麦克风作为独立目标时使用配对播放端写入；启用“设为系统默认麦克风”后，其他程序可使用 Windows 默认录音设备。
- 详情页本机监听音量只作用于 `AudioEngine` monitor；文件路由增益继续由路由页文件音源行控制。
- 音频快捷键字段由配置持久化，只在 Miccat 获得键盘焦点时触发；不是全局游戏热键。
- 设置侧栏可切换常规、音频设备、快捷键、虚拟线缆和关于分组；只显示现有后端能力。
- 音频库三栏用 6px `LibrarySplitter` 调整宽度，分类/详情可独立折叠与恢复宽度；中间列表至少保留 350px。分类树子层级每级缩进 10px，图标与名称同一垂直中线。
- 分类右键“编辑”打开现代化编辑对话框；分类图标保存在 `AppConfig.audio_category_icons`，重命名会迁移图标路径。
- 音频条目右键“编辑”只修改 `AudioEntry.name`、`icon_glyph` 和 `icon_path` 的软件配置，不重命名磁盘文件；对话框只提供默认音符、首字图标和用户选择的自定义图标文件。

## 6. 验证

```bat
scripts\check.cmd
scripts\smoke.cmd
target\debug\miccat.exe --snapshot-ui --page route --out route.raw
target\debug\miccat.exe --snapshot-ui --page route --source-audio-dialog --out route-audio-source.raw
target\debug\miccat.exe --snapshot-ui --page settings --out settings.raw
target\debug\miccat.exe --snapshot-ui --out player.raw
target\debug\miccat.exe --snapshot-ui --menu --out player-menu.raw
```

快照为原始 RGBA，文件头 8 字节是宽、高小端 u32；临时产物不要提交。
音频完整路径仍需手工验证：路由有声、音量有效、切歌/停止/拖动进度和右键动作。

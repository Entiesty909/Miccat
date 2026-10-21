## FRONTEND.md — Slint 界面边界与音频库契约

本文件记录当前 Slint 页面、桥接层与音频领域 API 的实际接缝。修改契约时，
同次更新 `ui/app-window.slint`、对应桥接回调和本文档。

## 1. 文件职责

`ui/app-window.slint`：窗口骨架与焦点作用域；纵向排列全宽顶栏、中央内容行、全宽底栏，并承载全局菜单遮罩。
`ui/components/app-header.slint`：58px 品牌操作栏；图标、Miccat 字样和版本靠左，三个操作按钮靠右并垂直居中；T 恤图片为 24px，大小与刷新图标一致且按按钮自身宽高居中。
`ui/components/sidebar-nav.slint`：45px 仅图标路由、音频库、设置侧栏；只占中央页面行高度，点击切换主页面，无 Tooltip。
`ui/components/status-bar.slint`：48px 底栏左侧错误/详情、中间状态，右侧按“音源入口—总音量滑块—诊断计数”排列；拖动时只显示无背景纯文字 Popup，由屏幕级光标坐标驱动，可跟随鼠标离开主窗口。
- `ui/themes/light.slint`、`dark.slint`、`pink.slint`：分别保存三套颜色 token；`ui/theme.slint` 只做主题索引调度。标准 `CheckBox`/`LineEdit` 由 `Palette.color-scheme` 同步黑白主题，所有自定义窗口/右键菜单/跑马灯弹层读取 Theme token。
主题预设弹层仅显示 2×2 风格色块，不显示文字标签；主题选择保存在 `AppConfig.theme_index`，下次启动时首帧恢复。粉色窗口底色不透明，原图作为窗口内背景绘制；三页非卡片内容区使用 72% 不透明浅粉遮罩，页面内容卡片独立使用 60% 不透明表面，字段为 92% 不透明；弹层仍使用 90% 不透明的 `Theme.card`。
`ui/overlays/context-menu.slint`：遮罩、分类/条目菜单、坐标钳制和动作转发。
`ui/overlays/diagnostic-panel.slint`、`ui/overlays/global-overlays.slint`：右侧抽屉式问题中心（问题列表 + 日志开关）、
`DiagState` 全局状态与顶层覆盖层宿主；状态栏与设置页直接读写该全局，不经 `app-window` 透传。
`ui/components/diagnostic-row.slint`、`ui/components/diagnostics-card.slint`：问题行/徽标与设置页诊断卡片。
`ui/pages/route.slint`、`ui/components/source-row.slint`：卡片按“图标—源信息—输出设备—音量—操作—启用转发”排列；音频文件行显示 SFX 槽位状态，并独立控制该槽位播放/暂停/停止。
- `ui/components/add-source-panel.slint`：按真实进程、音频库和物理麦模型添加路由源；音频/麦克风双栏各自成卡、不包额外底色容器；麦克风列表从首个真实设备起纵向排列，添加麦克风成功后关闭独立窗口；路由麦克风源行第一行固定显示“麦克风”，第二行灰色小字显示所选设备的 Windows 友好名称。普通“添加音频源”入口每次重置为第一个“进程”Tab；替换动作仍显式打开对应源类型。设置区可选择跟随全局输出或指定输出。
`ui/components/audio-source-list.slint`、`audio-source-details.slint`：分别显示可搜索的真实音频条目列表和所选条目的元数据/路由音量设置。
`ui/components/source-actions-menu.slint`：源卡片右侧更多菜单；由独立 `PopupWindow` 承载，按源类型提供替换进程、替换音频、替换麦克风、删除和单独转发。
`ui/components/category-sidebar.slint`：真实 `/` 分类路径、计数、拖拽排序、右键编辑和分类图标，图标与名称使用同一横向布局。
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
- 刷新进程/设备按钮在 worker 枚举所有列表，50 ms 状态定时器把结果写回 Slint；设备分类开关走同一刷新路径。
  - Process、file、mic 各自的 render ID 保存到 `OutputConfig`；进程和文件卡片额外按槽位保存独立 render ID，启动时 `SourceOutputRoutes` 将每个输入送到自己的 render 端点。
  - 进程行开关只切换当前进程的启用状态，关闭后保留卡片；删除动作才从路由源集合移除进程。
  - 文件行启用只代表路由配置，不启动播放；全局或单独转发启动后仍保持静音，由用户点击卡片播放按钮启动对应 SFX 槽。
  - 文件播放/暂停/停止只操作当前卡片的 SFX 槽；停止后按钮显示播放三角，暂停时点击恢复原位置。
  - 关闭文件源转发会停止该槽播放；重新启用不自动续播。文件源状态与音频库全局播放器互不混用。
  - 更多菜单的“单独转发”只打开选中的一个源，使用该源保存的输出设备，绕过底部总开关的多源选择。
-  麦克风和进程源提供删除/单独转发；快捷键只对音频文件启用，避免伪造没有配置模型支撑的进程热键。
-  添加源麦克风的“添加麦克风”按钮走独立提交回调：先同步选择到主路由、再隐藏添加源窗口；列表内切换设备仍只修改选择，不触发窗口关闭。
-  添加/替换进程前检查 PID 是否已在其它卡片；添加/替换音频前检查音频库实际条目下标，重复项拒绝提交并保留当前替换目标。
-  添加源麦克风的输出目标保存到 `AppConfig.output.mic_output_id`；默认“跟随全局默认设备”时清除单独目标。
-  输出设备下拉框的容器宽度固定；选中名称过长时只移动文字，使用往返横向滚动，不改变下拉框和列宽。
-  源操作状态由路由页统一维护，同一时间只显示一个源菜单；菜单展开时隐藏对应 `...` 触发按钮，点击另一源的菜单会切换到新菜单。
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
- 路由页进程源关闭时保留源行；文件源关闭会停止对应 SFX 槽，重新启用不自动续播。
- 源右侧更多菜单由独立 PopupWindow 承载，自动避开卡片/滚动容器裁剪；点外部关闭，菜单内点击执行操作后关闭。
  - 文件源启用、路由运行和文件播放是三个独立状态；开启全局/单独转发不自动播放文件，用户需手动点击卡片播放。
  - 文件路由卡片状态和播放控制均作用于对应 SFX 槽位；音频库全局播放器状态与路由卡片状态互不混用。
  - Process、文件、麦克风输出 ID 分别持久化；物理麦克风是捕获源而不是可写 render 端点。虚拟麦克风作为独立目标时使用配对播放端写入；启用“设为系统默认麦克风”后，其他程序可使用 Windows 默认录音设备。
- 详情页本机监听音量只作用于 `AudioEngine` monitor；文件路由增益继续由路由页文件音源行控制。
- 音频快捷键字段由配置持久化，只在 Miccat 获得键盘焦点时触发；不是全局游戏热键。
- 设置侧栏可切换常规、音频设备、快捷键、虚拟线缆和关于分组；只显示现有后端能力。
- 音频库三栏通过 `LibrarySplitter` 拖动条调整分类栏、列表栏和详情栏宽度；使用按下点差值避免抖动，右侧分隔条向右扩大中间列表。
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

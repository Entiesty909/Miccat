# Step 53 — 路由音频控制与添加源窗口收口

范围：同批收口路由三类卡片和文件播放控制。进程、音频、麦克风卡片按设计稿呈现左侧居中图标与源信息、文件播放区（仅音频）、输出设备与转发音量；进程预览音量为禁用占位，文件卡片第三行用于启用转发，进程/麦克风开关仍在右侧。进程副标题显示 PID，文件标题为音频库备注名、分类供悬停查看，麦克风副标题为设备友好名。右上角上三角接管原 `...` 菜单，约 150 ms 翻转后打开，运行中已启用的卡片不可展开。启用卡片保留蓝色左竖线，文件进度可拖放定位；模式按钮只显示图标，悬停提示当前模式。四种播放模式与分类上一曲/下一曲及自然结束续播按槽独立。添加源入口移到顶栏；删除路由页重复刷新进程和说明文字。虚拟设备双端冲突只校验启用卡片，滚动时覆盖层不抢麦克风按钮。底栏删除错误短句/详情按钮，问题中心保留诊断操作。淡粉主题将壁纸和页面遮罩后台预合成，窗口改变后防抖重算。未改变麦克风捕获、默认设备策略或全局刷新机制。

本批补充验收（2026-10-23）：进程/麦克风卡片的转发音量左侧图标点击后静音，再点击恢复原增益；卡片右侧独立静音按钮移除，滑块和标签不移动。麦克风卡片即使主窗口仍选择“不混入麦克风”哨兵，点击启用也先选择首个真实设备，再走原 `mic-mix-changed` 写入启用状态；已选真实设备保持不变。运行中进程/麦克风开关仍遵循原锁定规则。
本批文件播放补充验收（2026-10-23）：文件转发开关移到设置区第三行，窄窗口让出百分比显示宽度，进程/麦克风开关位置不动。未启用转发仍可本机试听、暂停、切歌与循环；播放中启用或启动路由时从当前进度接入，不自动播放原本静止的文件；关闭转发仅撤掉路由槽。播放器顶部控制条与卡片对同一活动文件的暂停/停止同步，不改变其它文件卡片。
本批路由输出补充验收（2026-10-23）：三类卡片原输出区域改为可展开的多选设备，按可见端点的稳定 ID 保存；配对扬声器与虚拟麦克风互不连带勾选，运行时才映射到同一 render。路由启动只收集已启用源的设备，禁用卡片的配对虚拟端点不进入冲突校验。单独启动只使用当前卡片目标。播放页文件默认设备单独保存，新建文件源采用该默认值，既有文件卡片互不覆盖；读旧 `file_output_id` 时迁移默认值与原首槽目标。

本批补充验收（2026-10-23，音量入口与试听）：音频库顶栏“添加音频”在最左，之后依次为四个上下居中的播放图标和本机试听滑块，搜索栏不再重复放添加按钮。文件卡片的四个播放按钮后增加 🔈，麦克风卡片原转发音量行也提供音量入口；悬浮层可分别调整转发音量与本机预览音量。文件本机增益按卡片槽位保存，麦克风本机增益独立于转发、默认 0 以防止启动时回授；进程声源与布局不改。

改动文件：
- `src/audio/sfx.rs`、`src/engine.rs`、`src/engine/{output,playback}.rs`、`src/cli/mod.rs`：为每个 SFX 槽发布播放状态和 100 ns 位置，保留独立暂停/恢复/停止；定点起播和代际校验避免旧解码覆盖新 seek。
- `src/config/mod.rs`、`src/ui_bridge/cache.rs`、`src/ui_bridge/persist.rs`：按文件槽持久化四种播放模式，并维持条目、模式、完成态数组对齐。
- `src/ui_bridge/callbacks/route/{audio_queue,sources}.rs`：按当前条目的精确分类实现前后切歌和自然结束续播；跳过其它卡片已占用条目，拖动进度按文件槽发起定位。
- `src/ui_bridge/{sources,status}.rs`、`src/ui_bridge/callbacks/{mod,route,route_start}.rs`、`src/ui_bridge/dev/snapshot.rs`：投影精简的三类源标题/副标题、分类、进度与模式；路由启动输出只收集启用的源。
- `ui/components/{source-row,slider-row,source-actions-menu,app-header,page-host,button,status-bar,diagnostic-row}.slint`、`ui/overlays/{context-menu,diagnostic-panel}.slint`、`ui/pages/route.slint`、`ui/app-window.slint`、`ui/theme.slint`：设计稿布局、图标模式与 Tooltip、短设备下拉、顶栏入口、进度拖放、底栏精简及问题中心重排；主题预合成图片按窗口尺寸切换。
- `src/ui_bridge/theme.rs`、`src/ui_bridge/theme/composite.rs`：主题图片异步预合成、防抖重算和停用后的过期回写拦截，其他独立窗口保持原背景。
- `src/ui_bridge/callbacks/route/{sources,tests}.rs`、`src/ui_bridge/cache.rs`：麦克风卡片跳过未混入哨兵并保留已选设备，麦克风静音缓存恢复增益；源卡片静音入口沿用原引擎回调。
- `ARCHITECTURE.md`、`BACKEND.md`、`API.md`、`FRONTEND.md`、`DEBUGGING.md`、`PROJECT.md`：同步队列、定位播放、输出冲突、卡片、诊断入口及粉色渲染契约。

- 本批音量入口续改：`ui/pages/{player,route}.slint`、`ui/components/{audio-transport,source-row,source-volume-panel,page-host}.slint`、`ui/app-window.slint` 接入顶栏与双音量悬浮层；`src/ui_bridge/{sources,mod}.rs`、`src/ui_bridge/callbacks/{player,settings,route,route_start}.rs`、`src/ui_bridge/callbacks/route/{audio_queue,sources}.rs`、`src/config/{mod,store}.rs` 保存并应用独立监听增益；`src/engine/{monitor,fanout,output}.rs` 和 `src/audio/render.rs` 提供物理麦本机监听分支。同步 `ARCHITECTURE.md`、`BACKEND.md`、`API.md`、`FRONTEND.md`、`DEBUGGING.md`。

怎么跑：
- `CARGO_TARGET_DIR=%TEMP%\miccat-agent-build scripts\build-dev.cmd fmt -- --check`
- `CARGO_TARGET_DIR=%TEMP%\miccat-agent-build scripts\build-dev.cmd build`
- `CARGO_TARGET_DIR=%TEMP%\miccat-agent-build scripts\build-dev.cmd test`
- `%TEMP%\miccat-agent-build\debug\miccat.exe --snapshot-ui --page route --out <临时文件>`
- `%TEMP%\miccat-agent-build\debug\miccat.exe --snapshot-ui --page route --source-menu-click --out <临时文件>`
- `%TEMP%\miccat-agent-build\debug\miccat.exe --snapshot-ui --diagnostics --out <临时文件>`
- `%TEMP%\miccat-agent-build\debug\miccat.exe --list-devices`

实测：2026-10-22，Windows x64。
- 默认目标目录的运行中 exe 被占用，使用用户临时目录中的独立 `CARGO_TARGET_DIR` 构建；`fmt -- --check`、Debug 构建通过，48 项单元测试通过。新增用例覆盖禁用卡片的配对端点不进入启动输出、双端启用提交两个 ID、定位后的绝对播放时间以及预合成矩形边界。
- 真正向软件窗口文件进度条发送鼠标按下、向右拖动、松开：槽位 1 回调输出 `seek-click 1 0.8159091`。独立 WAV 播放探针从 0.35 秒起播 0.5 秒，输出与对应源采样逐点比较为 0/18240 处不一致；该对比及纯逻辑测试不代替真实设备试听。
- 三类卡片 1280×800 截图检查：音频卡片启用时蓝色竖线出现、滑块可拖；菜单延时后出现，下方麦克风静音和启用分别收到槽位 2 的点击，运行中启用卡片的上三角无法打开菜单。最终重新构建后执行 `--snapshot-ui --page route`、`--source-menu-click`、`--diagnostics`，三张窗口截图渲染成功；`--list-devices` 枚举真实输出设备成功。
- 合成图用真实 `composite::render` 和 Slint 软件渲染对比 1551×739 帧：与旧的透明遮罩路径所有 RGBA 通道最大差值 2，顶栏像素相同。12 卡片、30 次滚动帧的本地合成图原型测量由 4.39/4.43 秒降至 4.09/4.11 秒（约 7%）；这是离屏代理指标，未在用户粉色主题物理窗口测量帧率，不能宣称彻底消除卡顿。ListView 虚拟化、图片减半、渲染缓存提示在相同原型中无稳定收益，未保留。
- `source-row.slint` 529 行与 `page-host.slint` 565 行高于 450 行审查阈值，分别负责共享卡片与页面桥接契约；`app-window.slint` 770 行高于 600 行硬阈值，属于原有主窗口装配超限，本批没有增加超限文件数；`diagnostic-panel.slint` 460 行、`sources.rs` 768 行仍保持既有超目标状态。本批不为数字拆空壳，后续需按职责切片处理装配文件。

2026-10-23 补充验证：`scripts\build-dev.cmd fmt`、`test`（49/49）及 `build` 成功；1280×800 路由快照已生成并检查，进程与麦克风音量图标位于转发音量行左侧，右侧原静音按钮已移除，滑块和启用开关仍在原位置。新增测试覆盖未选/已选/无真实麦克风设备时的启用索引。静音→恢复的真实设备听感、麦克风开关物理点击和桌面帧率未在本轮自动化验证。`check.cmd` 通过格式阶段后被未触及的 `src/diagnostics/{control,hub,layer,log}.rs` 中 16 处文档注释空行 Clippy 警告阻断；单独行数检查仍被未触及的 `ui/app-window.slint` 771 行历史硬上限阻断。本轮触及的 `source-row.slint` 478 行、`callbacks/route/sources.rs` 772 行超过审查阈值但低于 600/800 硬限，分别仍承担统一卡片和路由源回调；未增加硬上限文件数。历史注释覆盖仍未全仓完成，本轮仅同步碰过接口的注释。
2026-10-23 本批续验：`fmt`、Debug `build` 与 49 项测试通过，1280×800 路由快照确认文件开关处于第三行、转发音量百分比不重合，进程/麦克风保留右侧原开关。一次性双声道 PCM16 WAV 从 0.35 秒起播并消费 36480 个采样，对比文件为 0/36480 不匹配；真实用户 MP4 的同一探针产生 1873/37434 不匹配，压缩格式寻址未由此通过。真实设备的卡片试听与播放中切入输出尚未人工听感验收；快照、探针和单测不替代该场景。`source-row.slint` 518 行超过 450 行审查阈值，仍承担三类共用卡片与文件控制；未新增超过硬上限的文件。
2026-10-23 路由目标续验：`fmt -- --check`、Debug `build`、62 项单测通过；`--list-devices` 实际枚举 Realtek、VB-CABLE 与 Steam 的 render/录音端。1280×800 路由快照显示三张卡片的输出下拉仍占原位置，勾选目标友好名称在各自卡片上展示，文件开关位于音量下方且百分比不重叠。测试覆盖进程槽与文件槽多选隔离、最后一只目标不能误取消、禁用卡片不打开配对端点、配置旧单选迁移与保存回读。播放页默认设备另存 `file_default_output_id`，不再覆盖首个文件槽；输出回调拆至 `route/sources/outputs.rs` 后 `sources.rs` 为 708 行、输出模块 150 行，`source-row.slint` 为 516 行仍承担三类共享布局。真实虚拟设备同时转发与物理下拉点击/试听未在自动化快照中验证；历史注释遗漏与 `app-window.slint` 超硬阈值尚未由本切片解决。
2026-10-23 本批补验：`fmt -- --check`、Debug `build` 和 65 项测试通过；新增用例检查同 render 的虚拟扬声器/麦克风以各自可见 ID 单选与双选后，卡片勾选投影一致。`--list-devices` 在实机枚举 VB-CABLE、Steam 及物理设备；1280×800 路由快照与 698×620 添加文件/麦克风窗口快照确认源卡片仍在原位置、输出控件显示友好名。独立对话框多选提交和实际音频双端路由未用物理点击/听感验证。按设备身份映射职责拆出 `sources/output_selection.rs`、测试迁 `sources/tests.rs`，文件上下文页码映射回 `library.rs`；`sources.rs` 降至 778 行，未新增超过 800 行硬阈值文件。`source-row.slint` 519 行、`add-source-panel.slint` 569 行、`sources.rs` 778 行、`window_lifecycle.rs` 612 行均超过审查阈值；`library.rs` 510 行超常规目标但不达审查阈值。共享卡片、添加源窗口、源行投影、独立窗口生命周期和音频库索引各有职责，不为行数机械拆分。

2026-10-26 音量入口续验：`cargo fmt -- --check` 通过；经 `scripts\build-dev.cmd build` 的 Debug 构建通过；`scripts\build-dev.cmd test` 通过 66/66 项单测。`--snapshot-ui --page player --out %TEMP%\miccat-player-volume-final.raw` 与 `--snapshot-ui --page route --out %TEMP%\miccat-route-volume-final.raw` 均生成 1280×800 RGBA 快照并核对：音频库顶栏顺序为“添加音频 → 四个播放图标 → 本机试听滑块”，路由音频卡片四个播放按钮右侧有 🔈 入口，麦克风卡片原转发音量行右侧有 🔈 入口，进程卡片布局未改。双音量滑块的真实拖动、物理麦克风监听听感、转发与本机设备占用组合仍需桌面实机验收；快照和单测不替代该验证。
行数门禁复核：本批触及的 `src/ui_bridge/sources.rs` 709 行、`src/ui_bridge/callbacks/route/sources.rs` 785 行、`ui/components/page-host.slint` 560 行、`ui/components/source-row.slint` 581 行均为既有职责文件并超过审查阈值；未新增超过硬上限的文件。`scripts\check-lines.ps1` 仍被未触及的 `ui/app-window.slint` 769 行历史硬上限阻断。

2026-10-27 路由卡片音量入口精简（用户定点需求）：
- 进程卡片（kind=0）删除设置区「预览音量」整行（图标 + 禁用占位滑块 + 标签）；该行原为不可交互占位，不向 bridge 上报数值。
- 音频/文件卡片（kind=1）删除设置区「转发音量」整行（图标 + 滑块 + 标签）；转发音量改为仅由播放区 🔈 双音量浮层调节，设置区第三行的启用转发开关不动。
- 「转发音量」行改为 `if root.kind != 1` 包裹；原文件专属音量图标分支（kind==1）因不可达已删除，避免死代码。
- 仅删 UI 元素，未动 bridge 属性/回调：source-preview-volumes、preview-volume-edited、volume、volume-edited 仍由 🔈 浮层使用。
- 麦克风卡片 🔈 浮层（转发 + 本机预览双滑块 PopupWindow）与三类卡片「启用转发」开关的位置归属，记为待定设计方案，本轮未改代码（方案见对话，待用户拍板）。
实测：2026-10-27，`slint_build::compile`（build.rs:5）通过；`cargo build` 因本沙箱缺 Windows 10 SDK 的 rc.exe（vcvars 依赖的 reg.exe 被安全策略拦截）未能完成资源嵌入与链接。Slint 编译本身已验证通过，全量构建与 `--snapshot-ui` 复核待用户在本地 `scripts\build-dev.cmd build` 完成。

下一步：真实设备验证配对端点路由、分类续播、物理窗口的菜单悬停提示与粉色模式滚动手感；同批卡片调整继续更新本记录，不另建阶段文档。另：麦克风 🔈 浮层与三类卡片「启用转发」开关的位置方案待用户拍板，拍板前不改代码。

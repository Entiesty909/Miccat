# Step 53 — 路由音频控制与添加源窗口收口

范围：同批收口路由三类卡片和文件槽控制。进程、音频、麦克风卡片按设计稿呈现左侧居中图标与源信息、文件播放区（仅音频）、右侧固定宽度输出设备/转发音量及启用开关；进程和文件的预览音量仅为禁用占位。进程副标题只显示 PID，文件只显示音频库备注名、分类供悬停查看，麦克风副标题为设备友好名。右上角上三角接管原 `...` 菜单；约 150 ms 箭头过渡结束后才打开菜单，运行中已启用的卡片不可展开。启用卡片保留蓝色左竖线，文件进度条可从左向右拖放定位；不在播放或未知时长时不可拖。模式按钮只保留图标，悬停提示当前模式；路由页按钮均有悬停说明。四模式与分类上一曲/下一曲及自然结束续播按槽独立。添加音频源入口及原打开窗口逻辑移到顶栏，路由页删除“刷新进程”按钮及卡片上方说明文字，不留下空白。虚拟设备双端冲突只校验启用的卡片。滚动到下方麦克风时，覆盖层不会抢占按钮点击。底栏删除错误短句/详情按钮，问题中心抽屉重排列表/详情/日志但保留原操作。淡粉主题将壁纸与页面遮罩在后台预合成为一张不透明图，窗口尺寸改变后防抖重算；图不可用或重算期间沿用原渲染。未改变麦克风捕获、默认设备策略和全局刷新机制。

改动文件：
- `src/audio/sfx.rs`、`src/engine.rs`、`src/engine/{output,playback}.rs`、`src/cli/mod.rs`：为每个 SFX 槽发布播放状态和 100 ns 位置，保留独立暂停/恢复/停止；定点起播和代际校验避免旧解码覆盖新 seek。
- `src/config/mod.rs`、`src/ui_bridge/cache.rs`、`src/ui_bridge/persist.rs`：按文件槽持久化四种播放模式，并维持条目、模式、完成态数组对齐。
- `src/ui_bridge/callbacks/route/{audio_queue,sources}.rs`：按当前条目的精确分类实现前后切歌和自然结束续播；跳过其它卡片已占用条目，拖动进度按文件槽发起定位。
- `src/ui_bridge/{sources,status}.rs`、`src/ui_bridge/callbacks/{mod,route,route_start}.rs`、`src/ui_bridge/dev/snapshot.rs`：投影精简的三类源标题/副标题、分类、进度与模式；路由启动输出只收集启用的源。
- `ui/components/{source-row,slider-row,source-actions-menu,app-header,page-host,button,status-bar,diagnostic-row}.slint`、`ui/overlays/{context-menu,diagnostic-panel}.slint`、`ui/pages/route.slint`、`ui/app-window.slint`、`ui/theme.slint`：设计稿布局、图标模式与 Tooltip、短设备下拉、顶栏入口、进度拖放、底栏精简及问题中心重排；主题预合成图片按窗口尺寸切换。
- `src/ui_bridge/theme.rs`、`src/ui_bridge/theme/composite.rs`：主题图片异步预合成、防抖重算和停用后的过期回写拦截，其他独立窗口保持原背景。
- `ARCHITECTURE.md`、`BACKEND.md`、`API.md`、`FRONTEND.md`、`DEBUGGING.md`、`PROJECT.md`：同步队列、定位播放、输出冲突、卡片、诊断入口及粉色渲染契约。

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

下一步：真实设备验证配对端点路由、分类续播、物理窗口的菜单悬停提示与粉色模式滚动手感；同批卡片调整继续更新本记录，不另建阶段文档。

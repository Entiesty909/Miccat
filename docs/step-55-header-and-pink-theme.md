# Step 55 — 主窗口顶栏与淡粉主题收口

范围：将主窗口顶栏与三个页面、主题预设及淡粉背景、主题偏好保存、窗口/任务栏图标统一、图片 API 恢复和渲染优化作为同一批需求维护；后续 debug、视觉补充和 bugfix 更新本记录，不按对话次数新建 Step。粉色模式窗口本体保持不透明。

最终行为：
- 顶栏左侧为应用图标、Miccat 字样、版本；右侧为开始/停止、主题、刷新，内容垂直居中。T 恤图片 24px，与刷新 glyph 同一视觉尺寸；按按钮 x/y 居中，SVG 线宽维持 1.7。主窗、添加源窗、编辑窗标题栏和 exe 资源共用 `resources/icon.ico`。
- 主题菜单只显示三套纯色预设；`AppConfig.theme_index` 保存选择，启动首帧恢复，旧配置缺字段默认浅色。
- 淡粉窗口底色不透明；API 获取的原图满色绘制于窗口内。HTTP 非 200、HTTP 200 空响应和不可解码正文最多额外重试 3 次；WinHTTP 仍限制 8MiB 和超时，不增加依赖。
- 三页非卡片内容区使用 72% 不透明浅粉遮罩，提高空白区域文字对比度。页面卡片用独立 `Theme.content-card`（60% 不透明），字段 92%；弹出菜单等仍使用 90% 不透明的 `Theme.card`。
- 图片下载、解码、裁剪在后台执行；输出预裁剪为 1280×800 不透明位图，默认尺寸避免逐帧 `cover` 缩放。透明像素以淡粉色底合成，不再把不透明原图衰减到 18%。

改动文件：
- `ui/app-window.slint`、`ui/components/{app-header,button,page-host,card,source-row,audio-list}.slint`、`ui/pages/{route,player,settings}.slint`、`ui/theme.slint`、`ui/themes/pink.slint`、`ui/windows.slint`、`resources/{theme-shirt.svg,icon.ico,app.rc}`、`build.rs`。
- `src/platform/theme.rs`、`src/ui_bridge/theme.rs`、`src/config/{mod.rs,store.rs}`、`src/ui_bridge/mod.rs`。
- `FRONTEND.md`、`ARCHITECTURE.md`、`BACKEND.md`、`DEBUGGING.md`、`docs/step-02-rust-architecture.md`、`PROJECT.md`。

怎么跑：
- `scripts\build-dev.cmd fmt`
- `scripts\build-dev.cmd test`
- `scripts\build-dev.cmd build`
- `target\debug\miccat.exe --snapshot-ui --page route --out %TEMP%\miccat-route.raw`
- `target\debug\miccat.exe --snapshot-ui --page player --out %TEMP%\miccat-player.raw`
- `target\debug\miccat.exe --snapshot-ui --page settings --out %TEMP%\miccat-settings.raw`

实测：2026-10-22，Windows x64。
- 格式化完成；41 项单元测试通过，覆盖主题偏好兼容、HTTP 200 空响应最多 3 次重试、默认窗口裁剪及照片颜色保留。
- Debug 构建成功；路由、播放器、设置三页 1280×800 软件渲染快照均生成成功。路由快照确认 T 恤图标尺寸与刷新 glyph 相近并处于按钮中心。
- 实测背景 API 三次请求中两次返回 `404 text/html`（130 bytes），一次返回 `200 image/webp`（799036 bytes）；空图/错误状态走最多 3 次额外尝试。
- `--snapshot-ui` 不提供主题选择参数；未声称粉色模式的三页快照或桌面 CPU/FPS 已量化通过。
- `resources/icon.ico` 标题栏/任务栏绑定已纳入；软件客户区快照不覆盖 Windows 壳层图标，未声称完成标题栏/任务栏物理截图验收。

规模与历史记录：应用图标、主题菜单和淡粉背景多轮调整合并到本需求记录；历史注释覆盖仍未全仓完成，本阶段未清理无关旧代码。

下一步：桌面检查粉色模式下卡片透光、非卡片文字对比和帧率；同批问题继续补入本文件，不创建新 Step。

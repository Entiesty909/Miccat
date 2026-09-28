## FRONTEND.md — Slint 界面边界与音频库契约

本文件记录当前 Slint 页面、桥接层与音频领域 API 的实际接缝。修改契约时，
同次更新 `ui/app-window.slint`、对应桥接回调和本文档。

## 1. 文件职责

- `ui/app-window.slint`：窗口、页签、页面装配、状态栏；只实例化一次菜单覆盖层。
- `ui/overlays/context-menu.slint`：遮罩、分类/条目菜单、坐标钳制和动作转发。
- `ui/pages/route.slint`：进程/麦克风/输出选择、音量、电平和路由事件。
- `ui/pages/player.slint`：分类树、音频列表、播放条和菜单锚点；只上报事件。
- `ui/pages/settings.slint`：设备多选、混音、系统热键、试听开关和 VB-CABLE 安装。
- `ui/components/*.slint`：无领域权威的可复用控件；`empty-state.slint` 尚未接入。
- `ui/theme.slint`：颜色和尺寸 token。

Rust 桥接：

- `src/ui_bridge/mod.rs` 负责单实例、窗口装配、事件循环和退出 flush。
- `state.rs` 持有 `AudioEngine`、`AppConfig`、保存队列、热键和电平历史。
- `cache.rs` 持有筛选、索引、菜单/点击态和列表快照；仍有部分配置镜像字段。
- `callbacks/` 按路由、分类、条目元数据、播放和设置拆分回调。
- `src/library.rs` 提供分类路径、可见索引、递归变更和文件验证规则。
- 文件播放与试听通过 `AudioEngine` API；桥接不直接调用 decode、SFX 或 PreviewEngine。
- bridge 仍访问 `audio::device` / `audio::policy`、原生文件对话框和 VB-CABLE 安装。
- `SaveQueue` 以 300 ms 防抖串行写配置，退出时 flush。

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
- S6 已移除未接线的全局播放按键、音频条目热键和热键模式设置；实际热键只有 `toggle_route`。

## 4. 音频库语义

- 分类以 `/` 表示父子路径；“全部”和“未分类”默认存在并记录在 `AppConfig.audio_categories`，用户可删除。
- 删除“全部”清空软件内全部音频条目；删除“未分类”移除该分类及其后代条目；磁盘文件不删除。删除普通分类则把相关条目移到“未分类”。
- 导入音频归入当前选中的左侧分类；无选中分类时归入“未分类”。
- `src/library.rs` 负责后代匹配、路径展开、搜索后的可见索引和递归分类变更。
- 带缩进的显示文本是 UI 模型，不能写回 `AppConfig.audio_categories`。
- 可见索引按当前分类（含后代）和名称搜索映射回配置条目下标。
- `AudioEntry.duration_hns` 使用 100 ns；0 显示 `—`，导入和启动迁移时补齐。
- 同一条目在 450 ms 内再次点击视为双击，开始或停止播放。
- 进度来自 `AudioEngine::file_state()`；拖动时从目标位置重新启动文件流，暂没有独立 seek API。

## 5. 当前限制

- 音频条目列表尚无 `ScrollView` / `ListView` 虚拟化；长列表滚动体验待改善。
- 菜单高度固定为分类 132 px、条目 292 px，定义在 `context-menu.slint`。
- `--snapshot-ui --category-menu` 可渲染空分类树右键菜单；真实空白区域鼠标事件仍需桌面交互验收。
- 快照不能验证真实设备路由、试听听感、拖动进度和停止延迟；这些需要 Windows 桌面验收。

## 6. 验证

```bat
scripts\check.cmd
scripts\smoke.cmd
target\debug\mintcat-proc-mic.exe --snapshot-ui --out snap.raw
target\debug\mintcat-proc-mic.exe --snapshot-ui --menu --out snap-menu.raw
```

快照为原始 RGBA，文件头 8 字节是宽、高小端 u32；临时产物不要提交。
音频完整路径仍需手工验证：路由有声、音量有效、切歌/停止/拖动进度和右键动作。

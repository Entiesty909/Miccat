# API.md — CLI 与 Rust 内部契约

本项目没有 HTTP/REST/WebSocket API。本文记录用户可调用的 CLI 命令和进程内 Rust 模块接口；这些接口都属于单 crate，不承诺第三方兼容性。

## CLI

所有命令由 `src/cli/mod.rs` 派发。无参数启动 Slint UI；错误由组合根写到 stderr 并以非零状态退出。

| 命令 | 参数与默认值 | 可观察行为 |
|---|---|---|
| `--help` | `-h` 也可用 | 输出 CLI 用法 |
| `--list-processes` | 无 | 输出可选进程，每行含名称、PID 和可见窗口标题（若有） |
| `--list-devices` | 无 | 输出 WASAPI ID、Tab、设备友好名；包含当前过滤策略允许的输出端 |
| `--capture <pid>` | `--wav capture.wav`，`--seconds 5` | Process Loopback 捕获并写 PCM16 WAV |
| `--route <pid>` | 必须 `--device <wasapi-id>`；`--seconds 8` | 临时路由进程声；运行至少 0.5 秒后停止 |
| `--snapshot-ui` | `--out snap.raw`；`--page route\|player\|settings`，默认 `player`；`--source-dialog-click-tab process\|audio\|microphone` 模拟并断言类型卡切换；`--source-menu-click` 点击路由第一条源右上角上三角，等待 150 ms 后渲染弹层 | 软件渲染 UI RGBA 快照；菜单参数只能用于对应页面 |
| `--probe-playback <file>` | seconds=8, start=0, out=probe.wav | 逐采样探针 |

`--probe-playback` 可加 `--sfx` 或 `--raw`；非 raw 模式另写 `<out>.ref.wav`。

`--list-devices` 每行是 `<device-id>\t<friendly-name>`。
`--snapshot-ui` 文件头是宽、高两个小端 u32，后接 RGBA；分类菜单模式分发右键事件并断言菜单已打开。

`--snapshot-ui` 只在内存中补全快照 fixture，不因生成快照写回用户配置；输出仍为宽度、高度和 RGBA 数据。

## AudioEngine

`src/engine.rs` 的 `AudioEngine` 是内部音频运行时 API：

- `new()`：创建空引擎。
- `open_routed_outputs(ids, SourceOutputRoutes)`：为进程、文件、麦克风分别指定已打开 render 目标；相同 ID 复用一个目标，fanout 仅向对应目标写该源。
- `set_process_source(pid)` / `add_process_source(pid, slot)`：切换首个进程源或增加额外进程槽。
- `set_mic_source(Some(id))` / `set_mic_source(None)`：启用/关闭物理麦捕获输入。
- `set_volume(v)`、`set_mic_volume(v)`、`set_sfx_volume(v)`、`set_master_volume(v)`：设置各自 0–1 线性增益。
- `volume()`、`mic_volume()`、`sfx_volume()`、`is_running()`：读取运行期源增益/状态。
- `set_monitor_default(enabled)` / `set_monitor_volume(v)`：启用系统默认设备试听及设置试听音量。
- `play_file(entry, start_hns, target)` 启动音频库流；`PlaybackTarget::Both` 同时送路由和本机监听，`Monitor` 仅监听，`Route` 仅送活动路由。
- `stop_file()` 停止播放；`file_state()` 返回播放快照，位置单位为 100 ns。
- `play_file_source(entry, slot, looped, start_hns, paused)` 从 100 ns 媒体位置替换指定路由文件槽并保留暂停态；`file_source_state(slot)` 返回停止/播放/暂停/完成，`file_source_position_hns(slot)` 返回 100 ns 播放位置。同槽停止或再次 seek 会使较早的后台解码结果失效。
`stop()` 停止并回收路由与播放线程；路由需按顺序显式打开输出、设置进程源及可选麦克风源。

`EngineStatus` 值包括 `Idle`、`Running`、`ProcessExited`、`DeviceGone` 和 `Error(String)`。
音频库播放器 seek 由调用方停止后再次调用 `play_file()`；路由文件卡片的进度滑块只在播放/暂停态可拖动，松开后以目标位置再次调用 `play_file_source()`，不启动未播放的源。没有独立 `seek_file()` API。
音频源行的输出下拉绑定全局设备选择；多选输出 ID 保存于 `OutputConfig.device_ids`。
进程行启用只改变 `UiCache.disabled_pids`；文件行启用只改变对应槽位启用状态，不启动全局转发。文件卡片按所属精确分类处理上一首/下一首；四种模式保存在 `RouteConfig.audio_source_play_modes`，自然结束由 50 ms 轮询只处理一次。
源行静音为运行期操作，不改写持久化音量；源音量滑块更新 AudioEngine 增益和配置。Popover 峰值标记为 dBFS；增益 dB 为相对 100% 音量倍率。
内部命令通道、ring 和 WASAPI session 不属于 UI API。

## 音频库领域 API

`category_matches(entry, category)`：分类自身或任意后代匹配；`全部` 匹配所有条目。
- `visible_entry_index(entries, category, index)`：把当前分类及后代中的可见下标映射回配置下标。
`AudioEntry.duration_hns`、`file_size_bytes`、`sample_rate`、`channels` 为导入/迁移得到的文件元数据；`hotkey` 是可选的单键字符串。
bridge 以分类、大小写不敏感名称搜索、名称排序和每页 20 条计算可见顺序；所有行操作按同一顺序映射回配置下标。格式、采样率和码率来自真实文件元数据。
详情文件音量调用 `AudioEngine::set_sfx_volume()` 并防抖写入 `AppConfig.mix.sfx`；本机试听音量仍由 monitor API 单独控制。
- `category_paths(categories, entries)`：展开配置和条目中的 `/` 父子路径；根项是否存在由配置决定。
- `new_category(...)`：生成所选分类下不冲突的“未命名分类”名称。
- `rename_category(...)`：递归更新父分类、全部子节点和音频条目路径。
- `move_category(...)`：把整棵分类子树移动到目标分类下，或目标同级前/后；同步音频条目路径并拒绝循环/重名。
- 音频条目快捷键仅接受 `slint_key_to_vk()` 支持的单键（F1–F12、字母、数字、Space），在应用获焦时播放绑定条目；重复绑定及与路由热键相同的主键拒绝。
- 音频库列表按名称/分类筛选后再排序分页，分页大小为 20；列表和详情字段使用同一可见条目映射。
- `delete_category(...)`：普通分类递归删除并把条目移到“未分类”；删除“未分类”只移除软件内条目；删除“全部”清空软件内全部条目。均不删除磁盘文件。
- 导入音频归入当前选中的分类；配置版本 3 保留已删除根项状态，旧版本迁移时补入默认根项。
- `validate_file(path)`：检查支持扩展名及音频元数据，失败返回统一 `Error`。

## 配置与错误

`config::load()` / `config::save()` 操作 `%APPDATA%\\Miccat\\config.json`。
`AppConfig.theme_index` 保存 0 浅色、1 深色、2 淡粉；旧配置缺失该字段时使用浅色默认值。
保存失败只记 tracing；UI 修改经 `SaveQueue` 防抖，并在退出时 flush。

`Error::user_message()` 返回 UI 短句；底层 HRESULT/IO 原因通过 `Display` 和 tracing 记录。
不得将系统错误堆栈直接显示给用户。

## Slint 契约

UI 根组件为 `ui/app-window.slint` 导出的 `MainWindow`。
页面通过属性绑定和 callback 报告动作；Rust 注册位于 `src/ui_bridge/callbacks/`，修改契约要同步 `FRONTEND.md`。

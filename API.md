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
| `--snapshot-ui` | `--out snap.raw`；`--menu` 为条目菜单，`--category-menu` 为空分类树菜单 | 软件渲染 UI RGBA 快照 |
| `--probe-playback <file>` | seconds=8, start=0, out=probe.wav | 逐采样探针 |

`--probe-playback` 可加 `--sfx` 或 `--raw`；非 raw 模式另写 `<out>.ref.wav`。

`--list-devices` 每行是 `<device-id>\t<friendly-name>`。
`--snapshot-ui` 文件头是宽、高两个小端 u32，后接 RGBA；`--category-menu` 渲染空分类树下可新建、不可重命名/删除的菜单态。

## AudioEngine

`src/engine.rs` 的 `AudioEngine` 是内部音频运行时 API：

- `new()`：创建空引擎。
- `open_output(id)` / `open_outputs(ids)`：打开一个或多个 WASAPI render 目标。
- `set_process_source(pid)` / `add_process_source(pid, slot)`：切换首个进程源或增加额外进程槽。
- `set_mic_source(Some(id))` / `set_mic_source(None)`：启用/关闭物理麦捕获输入。
- `set_volume(v)`、`set_mic_volume(v)`、`set_master_volume(v)`：设置 0–1 线性增益。
- `set_monitor_default(enabled)` / `set_monitor_volume(v)`：启用系统默认设备试听及设置试听音量。
- `play_file(entry, start_hns)` / `stop_file()` 启停音频库流。
  `file_state()` 返回播放快照，位置单位为 100 ns。
- `start(pid, device, mic)` / `stop()`：CLI 路由便利入口及总停止。
- `status()`、`is_running()`、`sample_*_peak()`：读取状态、电平。

`EngineStatus` 值包括 `Idle`、`Running`、`ProcessExited`、`DeviceGone` 和 `Error(String)`。
文件 seek 当前由调用方停止后再次调用 `play_file()`；没有独立 `seek_file()` API。
内部命令通道、ring 和 WASAPI session 不属于 UI API。

## 音频库领域 API

`src/library.rs` 的函数为 `pub(crate)`，供 bridge 组合：

- `category_matches(entry, category)`：分类自身或任意后代匹配；`全部` 匹配所有条目。
- `visible_entry_index(entries, category, search, index)`：将筛选后的可见下标映射回配置下标。
- `category_paths(categories, entries)`：展开配置和条目中的 `/` 父子路径；根项是否存在由配置决定。
- `new_category(...)`：生成所选分类下不冲突的新子分类名。
- `rename_category(...)`：递归更新分类路径及条目路径。
- `delete_category(...)`：普通分类递归删除并把条目移到“未分类”；删除“未分类”只移除软件内条目；删除“全部”清空软件内全部条目。均不删除磁盘文件。
- 导入音频归入当前选中的分类；配置版本 3 保留已删除根项状态，旧版本迁移时补入默认根项。
- `validate_file(path)`：检查支持扩展名及音频元数据，失败返回统一 `Error`。

## 配置与错误

`config::load()` / `config::save()` 操作 `%APPDATA%\ProcessMic\config.json`。
保存失败只记 tracing；UI 修改经 `SaveQueue` 防抖，并在退出时 flush。

`Error::user_message()` 返回 UI 短句；底层 HRESULT/IO 原因通过 `Display` 和 tracing 记录。
不得将系统错误堆栈直接显示给用户。

## Slint 契约

UI 根组件为 `ui/app-window.slint` 导出的 `MainWindow`。
页面通过属性绑定和 callback 报告动作；Rust 注册位于 `src/ui_bridge/callbacks/`，修改契约要同步 `FRONTEND.md`。

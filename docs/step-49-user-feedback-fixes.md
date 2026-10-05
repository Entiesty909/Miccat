# Step 49 — 八项用户反馈修复

范围：修用户报的 8 个问题（开发脚本、目录清理、重复代码、线缆按钮、输出下拉、
播放/转发报错、实时切换、音频库点击区域）；没做：不支持运行中换输出设备（见下）。

## 改动文件

| 问题 | 改动 |
|---|---|
| 1 开发脚本 | 新增 `scripts/dev.ps1`、`dev.cmd`、`build-debug.cmd`、`build-release.cmd` |
| 2 目录清理 | 删除 4.9 GB 陈旧构建目录 `target-ui/`；`.gitignore` 增加 `/target-ui` |
| 3 重复代码 | `src/ui_bridge/sources.rs` 抽出 `SourceRows` + `apply()` + `add_file_row()` + `output_index()` + `set_if_changed()`，`dev/snapshot.rs` 改为复用 |
| 4 线缆按钮 | `src/vbcable.rs` 新增 `uninstall()`；`callbacks/settings.rs` 新增 `uninstall-cable`；`ui/pages/settings.slint` 已安装显示「升级到最新版 + 卸载」 |
| 5 输出下拉 | `src/ui_bridge/sources.rs` 加模型变更检测；`ui/components/source-row.slint` 行高按 kind 自适应；`ui/pages/route.slint` 改 `initial-output-index`；`callbacks/route/sources.rs` 回写模型 |
| 6 播放/转发报错 | `src/engine/monitor.rs` 重写为带设备记忆的 `MonitorState`；`src/engine.rs::stop()` 释放 monitor；`src/audio/render.rs` 新增 `describe_open_failure`；`src/engine/output.rs` 新增冲突折叠 |
| 7 实时切换 | `src/engine.rs` 新增 `replace_process_source`/`replace_mic_source`，`ProcessThread` 加 `slot`；`callbacks/route/sources.rs` 放开运行中门禁并热切换 |
| 8 列表点击 | `ui/components/audio-list.slint` 整行 `TouchArea` |

拆分：`callbacks/route.rs`（440 行）拆出 `callbacks/route/sources.rs`（291 行）。

## 根因与验证

### 6 + 7：0x8889000A（`AUDCLNT_E_DEVICE_IN_USE`）

用 `--probe-audio` 逐项复现（探针已在验证后删除）：

| 场景 | 修复前 | 修复后 |
|---|---|---|
| CABLE虚拟扬声器 + Realtek 同开 | ok | ok |
| CABLE虚拟扬声器 + CABLE In 16ch 同开 | `0x8889000A` | 可读原因 / UI 路径自动折叠 |
| 试听 → 播放 ×2 → 开分源路由 | `0x8889000A` | 正常启动 |

两个独立根因：

1. **试听引擎长期占用设备**：`AudioEngine::stop()` 从不释放 monitor，而
   `preview_enabled=true` 时启动即打开系统默认播放设备；随后转发对它 Initialize 就撞车。
   修复：`stop()` 调 `monitor::release()`；`set_default_monitor` 增加「同设备不重开」守卫
   （重开会把刚开始的解码流连同旧引擎一起丢掉，正是「第二次播放没声音」）。
2. **同一虚拟驱动的两个端点互斥**：实测 VB-Cable 的「CABLE虚拟扬声器」与「CABLE In 16ch」
   不能同时以共享模式打开。修复：按驱动接口名分组预检，UI 路径**自动折叠**到同一设备
   （转发照常工作），引擎层保留可读错误作为兜底。

判据演进（有实测支撑，不是猜的）：先试 `PKEY_Device_ContainerId` → 本机全为占位
`00000000-0000-0000-FFFF-FFFFFFFFFFFF`，不可用；改用驱动接口名
（`PKEY_DeviceInterface_FriendlyName`）→ 虚拟设备两端同名、物理设备互不相同，正确。

### 5：输出下拉点不动

两层叠加：① `sources::update` 每 50 ms 重建全部 `ModelRc`，而 Slint 的 `ModelRc`
相等性是**指针比较**，`for` 的 Repeater 因此每 tick 销毁重建所有行，下拉框刚点开就被换掉；
② 行内 `output-index` 是父级单向绑定，用户选择下一 tick 被模型值覆盖，且 Rust 回调只写
config、从不回写模型。修复：`set_if_changed` 逐项比较后再决定是否重建 + 回调回写模型。

布局变形：麦克风行（`kind == 2`）在固定 126 px 行高里多插一个 ComboBox，垂直溢出。
修复：行高按 kind 自适应（麦克风行 170 px），长文本加 `overflow: elide`。

### 3：重复代码

`dev/snapshot.rs` 与 `sources.rs` 有 5 处重复，其中「9 个 `set_source_*` 赋值」
与「9 个 Vec 声明」逐字节相同。抽出 `SourceRows` 后，快照复用真实现的写入逻辑，
不再依赖 `AppState`（快照没有真实引擎）。

## 怎么跑

```bat
scripts\dev.cmd                  :: 改代码自动重编+重启
scripts\dev.cmd -Release         :: release 模式
scripts\dev.cmd -NoRun           :: 只监听编译
scripts\build-debug.cmd
scripts\build-release.cmd
scripts\check.cmd
scripts\smoke.cmd
```

## 实测（2026-10-01，本机 Win11 x64）

- `scripts\check.cmd`：fmt + 严格 Clippy（`-D warnings`）+ 构建 + 行数门禁 **全绿**。
- `scripts\build-dev.cmd test`：**21 passed**。
- `scripts\smoke.cmd`：设备/进程枚举 + 4 张快照通过。
- `scripts\build-debug.cmd` → 打印 `target\debug\mintcat-proc-mic.exe`。
- `scripts\build-release.cmd` → 编译通过，`target\release\mintcat-proc-mic.exe`，**13.2 MB**。
- `scripts\dev.ps1`：`[scriptblock]::Create` 语法校验通过（未真跑监听循环）。
- 路由页 1280×800 快照：两行源均正常高度，各自显示正确输出设备，无变形。
- 设置页虚拟线缆分组快照：显示「VB-CABLE 已检测到」+「升级到最新版」+「卸载」。
- 冲突场景 CLI 实测：可读中文原因；折叠路径实测 `status=Running`。

行数门禁：`ui/app-window.slint` 基线由 383 抬到 **385**（新增 `uninstall-cable`
的根组件声明 + 转发共 2 行，属纯装配文件固有成本，见 `scripts/check-lines.ps1` 注释）。
另修：`check-lines.ps1` 因 PowerShell 5.1 按 ANSI 读取而误解析中文注释，已重写为 UTF-8 with BOM。

未实测：真实鼠标点下拉框/列表、UAC 提权卸载、真实听感（需人工）。

## 下一步

- 运行中换**输出设备**仍不支持（`open_outputs_inner` 开头 `self.stop()` 是硬打断点），
  换歌/换进程/换麦克风已支持热切换。
- 真实桌面交互需人工验收。

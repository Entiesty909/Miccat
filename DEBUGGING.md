# DEBUGGING.md —— 本仓故障定位手册

先分类再动手，别凭感觉改代码：

```text
设备/初始化问题 → --list-devices / --route 看端点与初始化
播放/进度问题   → `--probe-playback` 对齐解码流与 SFX/本机试听时钟，并忽略生产端启动静音
UI 渲染问题     → --snapshot-ui --out <file> [--menu | --category-menu] 出真 RGBA 图
错误/日志问题   → 问题中心（状态栏「N 个问题」或设置页「诊断」）；日志在 %APPDATA%\\Miccat\\logs\\miccat.log
```

---

## 0. 全局诊断系统

- 运行期警告/错误自动进入**问题中心**：状态栏右侧「⚠ N 个问题」按钮打开右侧抽屉，
  可看详情（含 `E-XXX` 诊断代码与 HRESULT）、建议动作、复制详情、移除单条、清空。
- 「问题中心 → 日志」标签页按 **11 个组件分类**（捕获/渲染/播放解码/设备/引擎/配置/热键/进程/界面/线缆/平台）
  单独设置日志级别，**改动即时生效**并写入 `AppConfig.debug`，重启后保持。
- 过滤规则：控制台固定 `info`（`RUST_LOG` 可覆盖）；**日志文件**按开关走；
  问题中心**无条件**记录 WARN/ERROR，开关关掉也看得见真问题。
- 桥接层（`ui_bridge`）的 WARN 不再重复进问题中心：由 `report_error` 统一给出用户短句、代码与建议。
- 日志文件超 2 MiB 自动截断；问题中心环形缓冲上限 200 条，相邻同源同标题只更新时间。

排障入口：

```bat
miccat.exe --snapshot-ui --diagnostics --out diag.raw
miccat.exe --snapshot-ui --diagnostics-log --out diaglog.raw
miccat.exe --snapshot-ui --diagnostics-badge --out badge.raw
miccat.exe --snapshot-ui --diagnostics-live --out live.raw
miccat.exe --snapshot-ui --page settings --section 5 --out diagset.raw
```

`--diagnostics-live` 不填示例条目，直接渲染当前真实捕获结果，用于验证「事件 → HubLayer → 问题中心」链路。

---

## 1. 通用顺序

1. 普通 `scripts\build-dev.cmd build` 的 Slint 错误在构建期报告；Live Preview 模式下语法错误由运行时 reload 检测，旧界面保持显示。
2. 用 CLI 把问题从 UI 里剥出来：`--list-processes` / `--list-devices` /
   `--capture <pid> [--wav out.wav] [--seconds N]` / `--route <pid> --device <id> [--seconds N]`。
3. 在能复现的最小路径上改；改完按 `AGENTS.md` 第 5 节真跑一遍（截图 / CLI 输出 / WAV 比对）。

## Slint Live Preview 开发

从仓库根运行 `./dev.ps1`。脚本设置 `SLINT_LIVE_PREVIEW=1` 并执行 `cargo build --features slint/live-preview`；首次会完整编译。保存 `.slint` 后由当前进程实时 reload，不触发监听器的 Rust 重编译。改 `.rs`、`build.rs` 或 `Cargo.toml` 会重新构建并重启。

Live Preview 只保留属性、模型和回调；改变 Rust 代码直接使用的 Slint 属性或回调 API 后可能退出，需要重新编译/启动。语法错误时前一版界面保持显示。正式版本使用 `scripts\build-release.cmd`，该命令不添加 Live Preview feature。

---

## 2. 音频链路

- 目标设备没声：先用 `--list-devices` 确认端点，再用 `--route` 验证目标。
  检查每目标 ring/render 线程和设备 Initialize 格式。
- `0x80070057`：检查 `WAVEFORMATEXTENSIBLE` 是否被截成 `WAVEFORMATEX`，导致 `cbSize` 尾部丢失。
  `sanitize_wave` 应生成 `cbSize=0` 的 PCM 或 IEEE float 格式。
- `0x88890021`：检查 Process Loopback Initialize flags 是否包含 `LOOPBACK`、
  `AUTOCONVERTPCM` 和 `SRC_DEFAULT_QUALITY`。
- `0x8889000A`（`AUDCLNT_E_DEVICE_IN_USE`）：设备已被占用。两个已知来源——
  ① 试听（monitor）持有系统默认播放设备且未被释放（检查 `AudioEngine::stop()` 是否走到
  `monitor::release()`）；② 同一虚拟驱动的两个端点被同时选中（如 VB-Cable 的
  「CABLE虚拟扬声器」与「CABLE In 16ch」）。判定按 `PKEY_DeviceInterface_FriendlyName`
  分组；路由启动会把冲突目标折叠到同一设备。
- 播放重复小片段：检查 `audio/decode.rs` 的 MF pending samples，
  再看 `audio/sfx.rs` / `audio/preview.rs` 是否只删除已消费的完整帧。
- 切歌/停止进度异常：检查 `AudioEngine::play_file()`、`stop_file()` 和 `file_state()`；
  状态回写由 `ui_bridge/status.rs` 完成。
- 本机试听无声：检查 `engine/monitor.rs` 的默认输出打开结果和 preview render 线程。
- 进程退出/设备拔出：确认状态更新为 `ProcessExited` / `DeviceGone`，相关线程收到 stop 并 join。

---

## 3. 音频库与 UI

- 分类异常：核对 `src/library.rs::category_paths()` 和按分类及后代展开的 `visible_entry_index()`；
  缩进显示文本不得写回配置。
- 分类树拖动后无法删除：先用 `--snapshot-ui --category-row-menu` 验证拖动后路径对应的行仍能打开右键菜单；
  删除回调必须按当前展开后的真实路径取值，不能复用拖动前的下标。
- 空分类树无法新建：检查分类卡片是否纵向填满、空白 `TouchArea` 是否收到右键并发出索引 `-1`；
  `--category-menu` 在空树 `(100, 300)` 分发右键事件并验证菜单打开；这是 Slint 事件回归，不是物理鼠标测试。
- 索引/时长对不上：核对当前分类及后代的可见索引映射与 `duration_hns` 补齐状态；名称搜索已移除。
- 菜单不消失或位置不对：检查 `ui/overlays/context-menu.slint` 遮罩、坐标钳制和页面上报；
  分类菜单常态 132 px、重命名编辑态 182 px、条目菜单 292 px。
- 页面渲染：`--snapshot-ui --page route|player|settings --out <file>`；菜单参数仅支持 player 页面。
- 跑马灯下拉：主页源卡片与添加进程对话框使用 `MarqueeComboBox`；检查 `ListView` 选项上的 `TouchArea.has-hover`、`Text.preferred-width` 和 `MarqueeText` Timer。关闭弹窗后选项子树应销毁，计时器停止。
- 长文本不滚动：先确认实测显示宽度大于选项视口，且鼠标确实悬停在该选项；短文本保持静止。
- 路由和设置页刷新共用异步列表刷新：worker 枚举进程/物理麦/输出设备，`status.rs` 在 UI 线程应用整组结果。
- 麦克风下拉变更通过 `mic-mix-changed` 保存当前 ID；选“不混入”时混麦自动关闭。
- 文件源更多按钮用播放文件路径映射当前筛选/分页；若目标不可见，不应改为对其他选中条目操作。
- 分类菜单使用 `--category-menu` 或 `--category-row-menu`，可验证空白区域和分类行命中。

---

## 4. 已知验证限制

- 原生 Slint 窗口不能用浏览器工具驱动；渲染可自动化（`--snapshot-ui`），**交互仍需 Windows 桌面手工操作**。
- 需要真实声卡/虚拟线缆的判断（听感、Discord 侧是否听到）无法自动化，必须人工确认。
- 工具与游戏权限不一致时热键收不到：与游戏同权限运行（都提权或都不提权）。
- S1–S6 已按职责拆分桥接、引擎、音频、CLI、platform、config 与 Slint 菜单；
  当前实现目录见根目录 `ARCHITECTURE.md`，旧单体路径不再存在。

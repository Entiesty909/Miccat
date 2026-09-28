# DEBUGGING.md —— 本仓故障定位手册

先分类再动手，别凭感觉改代码：

```text
设备/初始化问题 → --list-devices / --route 看端点与初始化
播放/进度问题   → --probe-playback 做逐采样比对（解码→SFX/试听时钟）
UI 渲染问题     → --snapshot-ui --out <file> [--menu | --category-menu] 出真 RGBA 图
```

---

## 1. 通用顺序

1. `scripts\build-dev.cmd build` 拿编译反馈（Slint 语法错误只在编译期暴露）。
2. 用 CLI 把问题从 UI 里剥出来：`--list-processes` / `--list-devices` /
   `--capture <pid> [--wav out.wav] [--seconds N]` / `--route <pid> --device <id> [--seconds N]`。
3. 在能复现的最小路径上改；改完按 `AGENTS.md` 第 5 节真跑一遍（截图 / CLI 输出 / WAV 比对）。

---

## 2. 音频链路

- 目标设备没声：先用 `--list-devices` 确认端点，再用 `--route` 验证目标。
  检查每目标 ring/render 线程和设备 Initialize 格式。
- `0x80070057`：检查 `WAVEFORMATEXTENSIBLE` 是否被截成 `WAVEFORMATEX`，导致 `cbSize` 尾部丢失。
  `sanitize_wave` 应生成 `cbSize=0` 的 PCM 或 IEEE float 格式。
- `0x88890021`：检查 Process Loopback Initialize flags 是否包含 `LOOPBACK`、
  `AUTOCONVERTPCM` 和 `SRC_DEFAULT_QUALITY`。
- 播放重复小片段：检查 `audio/decode.rs` 的 MF pending samples，
  再看 `audio/sfx.rs` / `audio/preview.rs` 是否只删除已消费的完整帧。
- 切歌/停止进度异常：检查 `AudioEngine::play_file()`、`stop_file()` 和 `file_state()`；
  状态回写由 `ui_bridge/status.rs` 完成。
- 本机试听无声：检查 `engine/monitor.rs` 的默认输出打开结果和 preview render 线程。
- 进程退出/设备拔出：确认状态更新为 `ProcessExited` / `DeviceGone`，相关线程收到 stop 并 join。

---

## 3. 音频库与 UI

- 分类异常：核对 `src/library.rs::category_paths()` 展开的路径与 `visible_entry_index()` 的过滤顺序；
  缩进显示文本不得写回配置。
- 分类树拖动后无法删除：先用 `--snapshot-ui --category-row-menu` 验证拖动后路径对应的行仍能打开右键菜单；
  删除回调必须按当前展开后的真实路径取值，不能复用拖动前的下标。
- 空分类树无法新建：检查分类卡片是否纵向填满、空白 `TouchArea` 是否收到右键并发出索引 `-1`；
  `--category-menu` 在空树 `(100, 300)` 分发右键事件并验证菜单打开；这是 Slint 事件回归，不是物理鼠标测试。
- 索引/时长对不上：核对列表筛选顺序（当前分类含全部后代 + 搜索）与 `duration_hns` 是否已补齐。
- 菜单不消失或位置不对：检查 `ui/overlays/context-menu.slint` 遮罩、坐标钳制和页面上报；
  分类菜单常态 132 px、重命名编辑态 182 px、条目菜单 292 px。
- 渲染/命中核对：`--snapshot-ui --out <file>`、`--menu` 条目菜单，或 `--category-menu` 空树右键模拟；
  后者未打开菜单会使命令失败，能够验证空白区命中路径。

---

## 4. 已知验证限制

- 原生 Slint 窗口不能用浏览器工具驱动；渲染可自动化（`--snapshot-ui`），**交互仍需 Windows 桌面手工操作**。
- 需要真实声卡/虚拟线缆的判断（听感、Discord 侧是否听到）无法自动化，必须人工确认。
- 工具与游戏权限不一致时热键收不到：与游戏同权限运行（都提权或都不提权）。
- S1–S6 已按职责拆分桥接、引擎、音频、CLI、platform、config 与 Slint 菜单；
  当前实现目录见根目录 `ARCHITECTURE.md`，旧单体路径不再存在。

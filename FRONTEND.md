# FRONTEND.md —— UI 边界、契约面与音频库状态

本文件描述**当前代码**的 UI 边界。改 UI 前先读它，改完必须同步它（`AGENTS.md` 第 5 节第 6 条）。
重构后的目标结构见 `docs/refactor-plan.md` 第三节。

---

## 1. 边界

| 文件 | 职责 | 明确不管 |
|---|---|---|
| `ui/app-window.slint`（S3 已拆分） | 窗口、顶栏页签、页面容器、常驻状态栏与菜单组件装配 | 页面内部布局 |
| `ui/pages/route.slint`（178） | 进程/麦/输出的选择与音量、声纹、事件上报 | 音频库 |
| `ui/pages/player.slint`（253） | 分类树、音频列表、时长列、播放条；只保存菜单坐标与开关 | 菜单本体（在 `ui/overlays/context-menu.slint`） |
| `ui/pages/settings.slint`（234） | 设备多选、混音、热键、试听、虚拟线缆 | 日常音频选择 |
| `ui/components/*.slint` | 可复用控件；`empty-state.slint` 目前无引用 | — |
| `ui/theme.slint`（30） | 颜色与长度 token；**尚无字号/字重/间距 token** | — |

Rust 侧 S1 现状：

- `src/ui_bridge/mod.rs` 负责启动装配、窗口事件循环与退出收尾。
- `state.rs` 统一持有运行期共享句柄；`cache.rs` 仍保留现有视图缓存字段。
- `callbacks/` 按路由、音频库、播放、设置拆分 38 个 Slint 回调。
- `lists.rs`、`library.rs`、`format.rs`、`persist.rs`、`status.rs`、`input.rs`、
  `window.rs`、`dialogs.rs` 分担桥接职责；播放辅助与快照分别在 `playback.rs`、
  `dev/snapshot.rs`。
- 配置修改以 300 ms 单次计时器合并，经串行后台线程保存；退出时强制 flush。
- S2 尚未落地：回调仍直接访问 `audio::{decode,sfx,preview}`，`PreviewEngine` 仍由
  `AppState` 持有。

---

## 2. PCM 与电平

- UI **不持有 PCM**：只持有三路各 32 点的峰值历史
  （`capture-wave` / `mic-wave` / `render-wave`），每 50ms 用 `sample_*_peak()` 衰减采样后写入。
- 解码、混音、重采样、音量全部在领域层；UI 只消费标量（时长、播放位置、状态、错误短句）。

---

## 3. 契约面

- 改契约必须同次改完 `ui/app-window.slint` 与 `src/ui_bridge/callbacks/` 对应注册；
  根模块通过 `callbacks::register` 装配，不留悬空转发。
- 已知死面（重构 S3 收口，先别当例子抄）：

```text
属性：mic-volume、mic-wave、mic-name、has-mic、category-edit-name、
      duration-seconds、entry-edit-name、device-index
回调：open-settings、entry-edit-tags、entry-save-tags、mic-volume-edited
```

---

## 4. 音频库状态语义

- 分类配置用 `/` 路径表示父子关系；`全部` 与 `未分类` 是固定根项。
- `audio_categories()` 把路径展开成带缩进的**显示模型**，回调按同一顺序映射回真实路径；
  **不要把带缩进的显示文本写回配置**。
- 当前分类包含其所有后代分类；搜索在筛选后的条目上执行；列表索引从当前可见结果从 1 开始。
- 时长来自 `AudioEntry.duration_hns`（100ns 单位；0 显示 `—`），由 WAV 头或 MF 元数据补齐。
- 单双击语义：最近一次点击在阈值内视为双击（播放/停止），否则视为选中；搜索词变化会重置可见列表。

---

## 5. 已知缺陷（重构修，不要当特性保留）

- 音频库条目列表**没有滚动容器**（`ui/pages/player.slint:143-195`），条目一多就看不见；
  S3 加 `ScrollView`，并考虑 `ListView` 虚拟化。
- 全仓无虚拟化：分类树、条目列表、设备列表都是裸 `for`，一次实例化全部行。
- S6 已移除未接线的全局播放按键设置、音频条目热键和热键模式配置；唯一实际注册的热键是 `toggle_route`。

---

## 6. 验证

```text
scripts\build-dev.cmd build
target\debug\mintcat-proc-mic.exe --snapshot-ui --out snap-ui.raw [--menu]
target\debug\mintcat-proc-mic.exe --list-devices
```

`--snapshot-ui` 用软件渲染器把真实窗口渲染成 RGBA（文件头 8 字节 = 宽/高 小端 u32），
可核对布局、索引列、菜单——这是本仓唯一能自动化的 UI 证据。

交互仍需 Windows 桌面手工验收：右键建多层子树、重命名/删除父树、导入音频看索引与时长、
双击播放与再双击停止、拖动进度跳转、切换音频后进度归零、切页签后状态栏一致。

# Step 44 — 主页交互、路由卡片与全局热键收口

范围：同一批收口主页排版、源卡接线、播放/菜单语义、音频库工具栏与全局热键；并合并随后针对路由列表、添加源对话框、设备名称、源菜单和图标对齐的补充修复。未支持鼠标侧键热键、运行中换输出设备。

## 1 主页排版与交互

| 问题 | 根因 | 修复 |
|---|---|---|
| 右上/左下状态点与文字不齐 | `Rectangle` 无 `vertical-alignment`，在 `HorizontalLayout` 里贴顶 | 用 `VerticalLayout { alignment: center }` 包住圆点 |
| 卡片变形/空白 | 麦克风行在固定 126px 里多插一个 ComboBox，垂直溢出 | 行高按 kind 自适应（麦克风 158px、其余 108px）；长文本加 `overflow: elide`；列改 `min-width` + `preferred-width` |
| 音量/静音/启用按了没反应 | `mute-toggled` 在 `SourceRow` 与 `route.slint` 都声明了，但**行实例上从未绑定** | 补绑定；音量与静音回调回写模型 |
| 主页没有滚动条 | 源列表是裸 `VerticalLayout` | 换成 `ScrollView` |

回写模型是必需的：`sources::update` 每 50 ms 重建行，不回写就会把用户刚拖的
滑块、刚点的静音翻回旧值。

## 2 播放暂停语义

根因（`callbacks/player.rs`）：双击正在播放的同一条目走的是 **停止** 分支
（`stop_ui`），不是暂停；而 `player-pause-toggle` 的实现是「记住位置 → `stop_file`
→ 恢复时重新解码 + seek」，不是真暂停。

修复：
- 双击正在播放/已暂停的同一条目 → 改为调 `player_pause_toggle`（暂停/继续）。
- 恢复没声音的根因是竞态：`set_monitor_default` 是**异步**打开试听设备，而
  `play_file` 立刻解码投递，`monitor::play_stream` 在引擎未就绪时返回 Err 并被吞掉。
  新增 `AudioEngine::ensure_monitor_default()`（同步确保就绪），播放/恢复前调用。
- `PlaybackTarget::Monitor` 且投递失败时不再静默：改为可见错误。

## 3 右键菜单

重做音频条目菜单，分组 + 现代选项：

```text
播放（进路由 + 本机试听）
仅本机试听
添加到路由主页
──────────────
设置快捷键 / 快捷键：Ctrl+Alt+F1
清除快捷键
──────────────
在资源管理器中显示
编辑文件
从音频库移除          （危险色）
```

原来「播放到麦克风」的说法已过时：本程序写的是虚拟线缆的**播放端**，
Discord 选对应录音端，不是「播放到麦克风」。

## 4 全局热键设计（回答「需要设计什么」）

**三件事，按依赖顺序：**

### 4.1 绑定模型：单槽 → 多绑定表

原 `HotkeyServer` 只有 `HOTKEY_ID = 1` + 一个 `AtomicU64`，只能有一条绑定。
改为：

```rust
HotkeyServer::start(mode, BTreeMap<i32, Hotkey>, Arc<dyn Fn(i32) + Send + Sync>)
HotkeyServer::set_bindings(BTreeMap<i32, Hotkey>)   // 全量替换并唤醒同步
HotkeyServer::take_failures() -> Vec<BindingFailure> // 注册失败回报
```

- `WM_HOTKEY` 的 `wParam` 就是绑定 id（原代码完全忽略）。
- 同步改成「先删后加」的差异更新；改键从最长 200 ms 延迟变为立即（`PostMessageW` 唤醒）。
- 加 `MOD_NOREPEAT`，按住不再连发。
- **所有注册必须留在热键线程**（Win32 限制：热键绑定到创建窗口的线程）。

### 4.2 动作寻址：id 分区

```text
1 = 开始/停止转发   2 = 上一首   3 = 下一首   4 = 停止播放
1000 + 条目下标 = 音频条目的全局热键
```

回调只传 id，`ui_bridge/input.rs::dispatch` 翻译成动作。新增动作不用改热键层。

### 4.3 冲突检测：三层

| 层 | 手段 | 现状 |
|---|---|---|
| 自家重复 | `(mods, vk)` **全等**比较 | 原来只比 `vk`：`Ctrl+A` 与 `A` 误报，`Ctrl+A` 与 `Ctrl+Shift+A` 漏报 |
| 外部占用 | `RegisterHotKey` 失败 → `take_failures()` → UI 提示 | 原来只 `tracing::warn`，UI 无感知 |
| 吞键风险 | 无修饰键单键**拒绝**并提示加修饰键 | 原来允许，会把 `A`/`1` 从全局抢走 |

`resync_bindings(ui, server, config)` 是唯一重注册入口：换绑后立即同步并把失败回报 UI。

### 4.4 UI

设置页「快捷键」卡片四行：开始/停止转发、上一首、下一首、停止播放；
每行「更改/重置」。录制时带修饰键（`AppFocusScope` 传 `event.modifiers`）。

**上/下一首的作用域**：`player_next/previous` 基于当前筛选/排序/分页的可见序列，
到本页边界停。全局热键复用同一语义。

### 4.5 RegisterHotKey 的硬限制（已实测/文档确认）

| 限制 | 说明 |
|---|---|
| F12 | 系统保留给调试器，**不可用** |
| Win 键组合 | 保留给操作系统 |
| 多键序列（Ctrl+K, Ctrl+C） | 不支持，只能单键 + 修饰符 |
| 鼠标侧键 | 不支持，需 `WH_MOUSE_LL` |
| 无修饰键单键 | API 允许但会全局吞键 → 本实现拒绝 |
| 每进程 id | 0x0000–0xBFFF |

## 同批路由卡片与添加源后续修正

- 路由源列表填满主内容区并使用 ScrollView；底部添加卡片移除，添加源按钮回到标题行并打开统一进程/音频/麦克风入口。
- 源卡统一列宽和高度，卡片状态/音量/静音/输出绑定回桥接；操作菜单支持单独转发、定位、快捷键和删除，类型卡和源菜单只显示有效动作。
- 设备下拉固定宽度，长名称限制在视口滚动；源菜单触发按钮显隐在后续路由迭代中进一步校正，当前行为按独立多源路由记录。
- 添加源页面保留真实进程图标与麦克风捕获设备列表；“添加音频源”按钮恢复到标题行，与刷新按钮并列。
- 源图标的居中修复经历布局包裹、整卡高度定位，最终将图标块移出 HorizontalLayout 并使用相对卡片的绝对 y 坐标，消除麦克风下拉导致的偏移。
- `resources/icon.ico` 仅用于系统窗口/exe；路由进程/文件图标使用真实 exe/Shell 图标，缺失时回退字体图标。

涉及文件：`ui/pages/{route,player}.slint`、`ui/components/{source-row,source-actions-menu,add-source-dialog,app-header,status-bar,button,page-host}.slint`、`src/platform/icon.rs`、`src/ui_bridge/{sources,library,status,cache}.rs` 及 route/library callbacks、`src/config/mod.rs`、`Cargo.toml`、`resources/`、`build.rs` 和 UI 契约文档。
## 改动文件

`ui/components/{app-header,status-bar,source-row,hotkey-card,app-focus-scope}.slint`、
`ui/pages/{route,settings}.slint`、`ui/overlays/{context-menu,global-overlays}.slint`、
`ui/app-window.slint`、
`src/hotkey.rs`（重写为多绑定）、`src/config/mod.rs`（`Hotkeys` 三个新字段 + `AudioEntry.hotkey_binding`）、
`src/ui_bridge/{input,library}.rs`、`src/ui_bridge/callbacks/{player,settings,library_view,library_entries}.rs`、
`src/engine/monitor.rs`（`ensure_default_monitor`）、`src/engine/playback.rs`。

## 实测（2026-10-01，本机 Win11 x64）

- `scripts\check.cmd`：fmt + 严格 Clippy（`-D warnings`）+ 构建 + 行数门禁 **全绿**。
- `scripts\build-dev.cmd test`：**21 passed**。`scripts\smoke.cmd`：通过。
- **暂停/恢复探针**（真实解码 + 真实设备，用后删除）：
  `播放 → pos=1.2s` → 暂停（`playing=false`）→ 恢复 → `pos=2.4s`（继续而非重播）
  → 再次从头播放 → `pos=1.2s`，全程 `error=None`。
- 渲染核对：路由页卡片（紧凑、列对齐、滚动条、无变形）、设置页四行热键、新版右键菜单。

行数：`ui/app-window.slint` 374/385（基线内）；新增 `ui/components/hotkey-card.slint` 承担快捷键卡片，
`settings.slint` 回到 287 行。

未实测：真实鼠标拖滑块/点静音、真实全局热键按下（需在游戏/其它程序前台）、
UAC 提权卸载、真实听感。这些需要人工确认。

## 同批后续验证（路由卡片与添加源修补）

- 路由/添加源更新多次使用独立 `CARGO_TARGET_DIR` 构建及 `check.cmd`/`smoke.cmd`；三种源页、菜单和声纹快照成功，严格 Clippy、21 项单测通过。
- 源图标居中经过多轮定位后，最终将图标块移出 HorizontalLayout 并按整张卡片绝对 y 定位；快照确认进程和文件源图标居中。
- 快照证明 Slint 结构、列宽与静态位置；真实鼠标点击、长列表滚动和设备听感仍需桌面/音频设备验收。

同批后续实测（2026-10-03 至 2026-10-11）：
- 添加源顶部入口、ScrollView、源行操作与长设备名下拉均通过独立构建和路由快照；多次 `check.cmd`/`smoke.cmd` 通过。
- 源图标位置已按整张卡片绝对居中；真实桌面菜单点击、长列表滚动和图标文件操作仍需设备验收。
- 真实桌面菜单点击、长列表拖动和图标文件手工操作仍按设备验收，不以重复快照替代。

## 下一步

- 运行中换输出设备仍不支持（`open_outputs_inner` 开头的 `self.stop()` 是硬打断点）。
- 鼠标侧键 / 多媒体键需 `WH_MOUSE_LL` 与 `RegisterHotKey` 之外的路径。

# Step 56 — 设置页单页化 + 功能收敛 + 问题中心抽屉可读性重构

本步为同一功能批次的连续工作（形态改造 → 功能收敛 → 关联抽屉可读性），统一记录于此。

## 一、设置页改为 QQ PC 风格单页（左固定导航 + 右单页滚动 + scroll-spy + 隐藏滚动条）

范围：设置页交互形态改造。左侧固定标签栏不随右侧滚动；右侧所有设置项在同一 ScrollView 内纵向单页排列；点击左侧标签自动把右侧对应分组滚到可视区顶部；右侧上下滚动时左侧高亮随当前可见分组自动更新（scroll-spy）；滚动条彻底隐藏（vertical/horizontal-scrollbar-policy: always-off）。

改动文件：
- `ui/pages/settings.slint`：重写为单页结构。左侧 `Card`（宽 200px）固定标签，`clicked => { root.goto-section(N); }`。右侧 `ScrollView` 隐藏滚动条，内含 `content` 纵向布局，命名分组 `sec-general/sec-audio/sec-hotkey/sec-cable/sec-about/sec-diag`。新增：
  - `property <[length]> section-tops`：各分组相对内容顶部的 y 坐标，用于点击跳转与 scroll-spy。
  - `property <int> active-section`：纯嵌套三元表达式，由 `scroll-view.content-y`（及 `visible-height`/`content-height`）推导当前激活分组；滚到最底强制选中末节。
  - `in-out property <int> target-section` + `changed target-section => { root.goto-section(root.target-section); }`：外部/左侧跳转目标，值变化时驱动右侧滚动。
  - `callback goto-section(int)`：写 `scroll-view.content-y` 实现程序化滚动（1.18 无 `scroll-to`）。
  - 所有原 `in-out` property 与 callback 保留不变。
- `ui/components/page-host.slint`：移除 `settings-section-index`；新增 `in-out property <int> settings-target-section` 与 `callback goto-section(int)`；在 `if root.page == 2: settings-page := SettingsPage { … }` 内绑定 `target-section: root.settings-target-section;`。
- `ui/app-window.slint`：移除 `settings-section-index`；`settings-requested(section) => { root.page = 2; self.goto-section(section); }` 经 `page-host` 路由；把 `PageHost` 由匿名实例改为具名 `page-host :=`，根上新增 `callback goto-settings-section(int)`（处理器置于布局之后）。
- `src/ui_bridge/dev/snapshot.rs`：移除已失效的 `ui.set_settings_section_index(...)`，改为 `ui.invoke_goto_settings_section(settings_section);`。

### 修复：Rust 侧遗留调用导致的编译错误
- 根因：设置页改单页后 `settings-section-index` 被移除，但 `snapshot.rs:157` 仍调旧 setter。沙箱 `cargo check` 只跑到 `build.rs`（rc.exe 缺失）就中断，rustc 从未执行，该错误在沙箱不可见。
- 修复：接好 `goto-settings-section` 回调 + 具名 `page-host` 新链路。

### 修复：点击左侧标签不跳转、滚动时左侧高亮不更新
- 根因：**`ScrollView.content-y` 符号与直觉相反**。fluent 风格里合法区间为 `[-(content-height - visible-height), 0]`，向下滚动 `content-y` 为负，正向滚动距离等于 `-content-y`。原写法写正值越界被夹回 0 → 点击不跳转；scroll-spy 用 `content-y`（≤0）与正 `section-tops` 比较 → 永远不成立。
- 改动：新增 `property <length> scroll-offset: -scroll-view.content-y;`，`active-section` 改用正向滚动距离比较；`goto-section` 改为 `scroll-view.content-y = -min(root.section-tops[i], max-scroll)`，`max-scroll = max(content-height - visible-height, 0px)`。
- 备注：`always-off` 只隐藏滚动条、不影响 `content-y`，故「不要滚动条」仍由 `always-off` 满足。

### 关键技术约束（Slint 1.18）
- 隐藏滚动条用 `ScrollBarPolicy.always-off`；程序化滚动写 `content-y`。
- 命名元素在属性表达式里用裸名 `sec-general.y`（不能用 `root.xxx`/`parent.xxx`），且声明需在该属性声明点已存在树中，故 `section-tops` 声明在布局之后。
- 纯属性绑定体禁止 `let`/赋值，改用纯嵌套三元表达式。
- 条件挂载（`if root.page == 2`）的实例 id 块外不可见，外部跳转只存目标值由 `changed` 自行滚动。

## 二、设置页功能收敛与热键落地

收敛设置页里"多余 / 没用"的功能，把快捷键的"上一首 / 下一首"真正落地到转发任务，并把诊断界面入口收敛到唯一来源（设置页）。

### 删除
- 「常规」分组及左侧导航按钮（用户认定多余）。
- 「音频设备」里的：输出设备 CheckBox 列表、`显示全部播放设备`、`设为默认麦克风（停止转发后还原）`（设备选择本就是路由页职责，设置页这一份冗余）。
- 右下角（状态栏）诊断入口徽标（`if DiagState.count > 0 { … ⚠ N → DiagState.toggle() }`）。

### 修改（语义）
- `上一首` / `下一首` 热键：`player.rs::navigate()` 原在音频库列表里上下切换；改为对**当前转发任务中启用的音频**切歌，多路启用则**同时切**。
- 分组数 6 → 5：`0=音频设备, 1=快捷键, 2=虚拟线缆, 3=关于, 4=诊断`，`section-tops` 数组同步 5 项。

### 保留
- 音频设备分组保留两个默认设备开关（`设为系统默认播放设备`、`设为默认通信播放设备`）：开关仅把策略持久化为 `output.set_default_render / set_default_communications` 配置标志，真实动作在路由启动后由 `route_start.rs` 应用——把**当前输出端口**设成系统默认播放 / 通信设备。多个去向的弹窗选择属于路由页职责，本步不改动。
- 后续末轮进一步收敛：删除「音频设备」卡片的 `dest-hint` 提示文本、「输出设备」标题文字、「刷新设备」按钮，卡片仅保留两个默认设备 `CheckBox`；删除「刷新设备」后 `root.refresh` 入口移除（设备枚举由路由页负责）。

### 改动清单（按文件）
- `ui/pages/settings.slint`：删除 `sec-general` 与左侧「常规」按钮；删除音频设备分组的设备 CheckBox 列表、`显示全部播放设备`、`设为默认麦克风`、后续再删 `dest-hint` 文案/「输出设备」标题/「刷新设备」按钮；移除设置页独占的 `devices`/`device-index`/`device-selected`/`all-devices`/`show-set-default-mic`/`set-default-mic` 属性与 `device-toggled`/`all-devices-toggled`/`set-default-mic-toggled` 回调；分组由 6 重排为 5，`section-tops`/`active-section`/`goto-section` 同步。
- `ui/components/page-host.slint`：设置挂载块移除上述属性 `<=>` 绑定与 3 个回调转发；`open-settings` 跳转由 `settings-requested(1)` 改为 `settings-requested(0)`。
- `ui/app-window.slint`：移除设置页侧桥接到根的属性/回调；保留根上路由页仍依赖的部分。
- `ui/components/status-bar.slint`：移除右下角 `if DiagState.count > 0` 的 ⚠ 诊断徽标，入口仅余设置页「诊断」。
- `src/ui_bridge/callbacks/player.rs`：`navigate` 改为分支——引擎运行且存在启用转发槽位时对每个启用槽位在 `audio_entries` 相邻条目间切歌（先 `stop_file_source` 再 `play_file_source` 重启该槽位转发，多路同时切，越界保持当前）；否则退回原库内列表导航。

## 三、问题中心抽屉可读性重构

重构「问题中心」抽屉的**问题**标签页布局，提升快速阅读体验；不改 Rust 桥接、不改 `DiagState` 全局契约、不改「日志」标签页。抽屉形态保留。

改动文件：
| 文件 | 改动 |
|---|---|
| `ui/components/diagnostic-row.slint` | 删除单行 `DiagRow`；新增 `DiagCard` 卡片组件（左侧级别竖条 + 标题 + 来源·时间 + 完整详情(等宽) + 建议 + 复制/移除）；保留 `SeverityDot`、`UnseenBadge` |
| `ui/overlays/diagnostic-panel.slint` | 问题页由「列表区 + 详情区」两区改为「筛选条 + 卡片流 + 底部操作」；新增 `filter`(0 全部/1 错误/2 警告/3 普通) 与 `match-filter` 纯函数；移除抽屉左边缘多余 1px 竖线；保留全部公开属性与 callback 签名、日志页不变 |

设计要点：
- 卡片流替代两区分离：原设计列表只显示标题+来源+时间，详情需点选后在固定区滚动，割裂；现每条诊断内联完整信息，单列滚动通读。
- 筛选条：全部/错误/警告/普通，右侧显示总量；`match-filter` 纯函数判定可见性，缺项按普通处理。
- 每张卡自带复制/移除：复用既有无参回调，不新增 Rust 接口。

## 四、Release 构建链接为 Windows 窗口子系统（避免双击弹出控制台黑框）

根因：项目未在 `Cargo.toml [profile.release]` 指定子系统，Cargo 默认把 exe 链接成控制台子系统，双击启动 Windows 强制分配控制台窗口（dev/release 均如此）。
修复：`Cargo.toml [profile.release]` 加 `windows_subsystem = "windows"` 在本机 cargo 版本下报 `unused manifest key`（不识别该 profile 字段），故改用 `build.rs` 在 `PROFILE == "release"` 时输出 `cargo:rustc-link-arg=/SUBSYSTEM:WINDOWS`；dev/debug 构建不输出该参数，保留控制台便于查看 tracing 日志。
- `scripts/build-release.cmd` 产出 exe 仅为软件本体、无控制台窗口；dev/debug 仍带终端。

## 验证（沙箱，缺 rc.exe 无法链接 exe）
- 各步均用「临时把 `build.rs` 里 `rc.exe` 的 `.expect` 改为缺失跳过，跑 `cargo check` 走完 rustc 全量检查，再还原 `build.rs`（`git diff build.rs` 为空）」的方式验证：Slint 与整 crate 均 `cargo check` **exit 0**，`cargo fmt -- --check` **exit 0**。
- 最终单一提交前统一验证：`cargo check` exit 0，`build.rs` 已还原。
- 渲染与交互（设置页跳转/scroll-spy/隐藏滚动条、两组默认设备开关生效、多路上/下一首同时切歌、问题中心卡片流与筛选、release 无黑框）需用户在 Windows 上 `scripts\build-dev.cmd build` / `scripts\build-release.cmd` 后实测验收。

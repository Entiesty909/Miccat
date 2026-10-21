# Step 42 — 全局错误诊断与调试输出系统

范围：做了全局问题中心（诊断汇总 + 详情 + 复制 + 打开日志）、运行时调试开关（按组件分类、
即时生效、持久化）、诊断日志落盘、状态栏问题徽标、设置页「诊断」分组；
明确没做：日志文件按天轮转（只按 2 MiB 截断）、把诊断导出为文件、网络上报。

## 改动文件

新增：

| 文件 | 职责 |
|---|---|
| `src/diagnostics/mod.rs` | 模块入口、日志安装、全局 hub/control 访问器 |
| `src/diagnostics/control.rs` | 调试开关中心：分类注册表、级别、EnvFilter 热重载 |
| `src/diagnostics/hub.rs` | 诊断环形缓冲（容量 200、相邻重复合并、未读计数） |
| `src/diagnostics/layer.rs` | tracing Layer：把 WARN/ERROR 写入问题中心 |
| `src/diagnostics/log.rs` | 日志落盘 `%APPDATA%\ProcessMic\logs\processmic.log`，超 2 MiB 截断 |
| `src/ui_bridge/diagnostics.rs` | 桥接：快照灌入 Slint 全局、面板/调试开关回调、`report_error` 等 |
| `src/platform/clipboard.rs` | Win32 剪贴板文本写入（复制详情） |
| `ui/overlays/diagnostic-panel.slint` | 右侧抽屉式问题中心（问题列表 + 日志开关两个标签页） |
| `ui/overlays/global-overlays.slint` | `DiagState` Slint global 与顶层覆盖层宿主 |
| `ui/components/diagnostic-row.slint` | 级别圆点、未读徽标、单条问题行 |
| `ui/components/diagnostics-card.slint` | 设置页「诊断与日志」卡片 |

修改：`src/main.rs`（安装日志、启动横幅）、`src/error.rs`（`code()` / `hint()` / `detail()`）、
`src/config/mod.rs`（`DebugConfig`）、`src/ui_bridge/{mod,status,lists}.rs`、
`src/ui_bridge/callbacks/{player,library_entries,library_view,settings}.rs`、
`src/ui_bridge/callbacks/route/{refresh,route_start}.rs`、`src/ui_bridge/dev/snapshot.rs`（`--diagnostics*` / `--section`）、
`src/cli/mod.rs`、`ui/app-window.slint`、`ui/components/status-bar.slint`、`ui/pages/settings.slint`、
`Cargo.toml`（`windows` 增加 `Win32_System_{DataExchange,Memory,Ole,SystemInformation}`、`tracing-subscriber` 增加 `registry`）、
`scripts/check-lines.ps1` 与 `AGENTS.md` 3.1.2（「设计更优可酌情抬基线」条款）。

## 设计要点

- **不在每个方法里手写开关判断**：调用点照常写 `tracing::debug!` 等宏，开关由 `EnvFilter`
  按模块 target 决定；只有字段构造很贵时才用 `tracing::enabled!` 挡一层。
- **两级过滤**：控制台固定 `info`（`RUST_LOG` 可覆盖）；日志文件按用户开关（master + 每分类覆盖）；
  问题中心无条件捕获 WARN/ERROR，保证开关关掉也看得见真问题。
- **桥接层不重复上报**：`HubLayer` 跳过 `mintcat_proc_mic::ui_bridge` target，由 `report_error`
  统一给出用户短句、诊断代码与建议动作。
- **状态放 Slint global**：诊断状态放 `DiagState`，状态栏与设置页直接读，不再逐层透传 15 个属性；
  app-window 因此从 397 行降到 381 行。
- **防刷屏**：环形缓冲上限 200；相邻同源同标题诊断只更新时间；日志文件超 2 MiB 截断。

## 怎么跑

```bat
scripts\check.cmd
scripts\smoke.cmd
target\debug\mintcat-proc-mic.exe --snapshot-ui --diagnostics --out diag.raw
target\debug\mintcat-proc-mic.exe --snapshot-ui --diagnostics-log --out diaglog.raw
target\debug\mintcat-proc-mic.exe --snapshot-ui --page settings --section 5 --out diagset.raw
target\debug\mintcat-proc-mic.exe --snapshot-ui --diagnostics-live --out live.raw
```

原始 RGBA 头 8 字节是宽高小端 u32。`--diagnostics` 填示例条目并打开面板；`--diagnostics-log`
切到日志标签页；`--diagnostics-badge` 只显示状态栏徽标；`--diagnostics-live` 不填示例，
渲染当前真实捕获结果。

## 实测（2026-10-01，本机 Win11 x64）

| 项目 | 命令 | 结果 |
|---|---|---|
| 格式 | `scripts\check.cmd`（fmt --check） | 通过 |
| 严格 Clippy | 同上（`clippy -- -D warnings`） | 通过，0 警告 |
| 构建 | 同上 | 通过 |
| 行数门禁 | 同上 | 通过；超限文件仍只有 `src/audio/capture.rs`、`ui/app-window.slint`（均未增长） |
| 单元测试 | `scripts\build-dev.cmd test` | 21 passed |
| CLI 冒烟 | `scripts\smoke.cmd` | 通过，4 张快照正常 |
| 问题中心渲染 | `--snapshot-ui --diagnostics` | 抽屉、标签页、未读徽标（3）、级别圆点、详情（`E-DEV: render init …` + 建议）均正确 |
| 日志开关渲染 | `--snapshot-ui --diagnostics-log` | 「默认级别」+ 11 个组件分类下拉正常 |
| 设置页诊断分组 | `--snapshot-ui --page settings --section 5` | 「诊断与日志」卡片正常 |
| 状态栏徽标 | `--snapshot-ui --diagnostics-badge` | 显示「⚠ 3 个问题（3 新）」 |
| 真实捕获链路 | 写坏 `config.json` 后 `--diagnostics-live` | 徽标显示「1 个问题（1 新）」，证明 WARN 经 HubLayer 进入问题中心 |
| 日志落盘 | 运行 `--list-devices` | 生成 `%APPDATA%\ProcessMic\logs\processmic.log`，含启动横幅 |
| 级别生效 | 配置 `master=0` | 日志文件为空（横幅被过滤）；`master=3` + `capture=5` 恢复正常 |
| 剪贴板 | 一次性单元测试往返 UTF-16 文本（含中文与 HRESULT） | 通过；测试已删除 |
| 回归 | 渲染问题修复 | 原先多行标题撑破 52px 行高，现标题换行符归一为空格 |

未实测：真实鼠标点击面板按钮、系统剪贴板粘贴到外部程序、超 2 MiB 日志截断（需长跑）。
日志文件与问题中心为真实渲染/真实文件，非模拟。

## 下一步

- 真实桌面交互（点徽标、复制详情、切标签页）需人工确认。
- 若后续日志量增大，可加按天轮转；当前只有大小截断。

# FILE-CONVENTIONS.md — 目录、命名与交付规范

本文件是仓库导航和约定说明。硬约束以 [`AGENTS.md`](AGENTS.md) 为准；冲突时不得用本文降低 AGENTS 的要求。

## 1. 模块与目录

- 单 crate；用 Rust 模块分层，不按类或方便程度堆在一个文件。
- 每个文件只承担一个清楚的职责；新模块必须有顶层 `//!` 说明。
- `foo.rs` 可配 `foo/` 子目录拆职责；拆分必须迁移调用点，不留双实现或兼容壳。
- 当前目录职责见 [`ARCHITECTURE.md`](ARCHITECTURE.md)，API 与状态边界见 [`BACKEND.md`](BACKEND.md)。
- `ui/app-window.slint` 负责窗口/全局覆盖层；三页装配与回调转发放 `ui/components/page-host.slint`。
- Slint 页面只编排视图；可复用控件进 `ui/components/`，覆盖层进 `ui/overlays/`。

## 2. 命名与接口

- Rust 类型使用 `UpperCamelCase`；函数、字段、模块使用 `snake_case`。
- Slint 属性/回调使用 `kebab-case`；Rust 回调注册按领域放在 `ui_bridge/callbacks/`。
- 模块内部优先 `pub(super)` / `pub(crate)`；仅跨 crate API 才使用 `pub`。
- 所有 `.rs` 与 `.slint` 文件必须有中文职责注释：Rust 文件顶层 `//!`；每个结构、枚举变体、实现块、字段、函数/方法和关键内部机制都必须有注释。Slint 的导出组件、property、callback、function，以及 Timer/TouchArea/PopupWindow 的关键事件和条件分支同样必须注释。
- 注释写用途、不变量、单位、生命周期、数据来源、线程/所有权/FFI 前提或交互副作用；关键代码注释解释原因，不复述代码。私有成员也必须覆盖；旧代码未覆盖时必须在阶段文档列出，不能宣称全仓完成。
- 注释用中文；标识符英文。WASAPI 设备 ID 当不透明字符串，不当 MME 下标。
- 状态只由权威对象持有：`AudioEngine` 管音频运行态，`AppConfig` 管持久化；UI 缓存仅放显示/交互派生态。

## 3. 文件规模与排版

- `.rs` ≤ 400 行、`.slint` ≤ 300 行是常规目标；不是机械拆分硬阈值。
- `scripts/check-lines.ps1` 对 `.rs` 超过 600 行、`.slint` 超过 450 行给非阻断审查警告；超过 800/600 行才失败。
- 超过常规目标时检查职责边界；装配、桥接契约和 FFI 边界若保持清晰，可保留并在阶段文档记录。
- `src/library.rs` 行为逻辑与 `src/library/tests.rs` 单元测试分离；测试作为子模块访问私有实现，不增加业务 API。
- `src/library.rs` 生产逻辑应保持在 400 行内；引擎与 capture 的历史超限基线由 `scripts/check-lines.ps1` 约束为不增长。

## 4. 依赖与 unsafe

```text
ui/ → ui_bridge/ → engine/ → audio/
                  ├→ library.rs / config/
                  └→ platform/、process、hotkey
```

- `engine/` 和 `audio/` 不得依赖 Slint；`process/` 不得依赖 `audio/`。
- `ui_bridge/` 不得直接调用 `audio::decode`、`audio::sfx` 或 `audio::preview`；使用 `AudioEngine` 安全 API。
- 设备枚举、COM 和文件对话框只放在基础设施职责中；跨层例外必须在架构文档中说明。
- `unsafe` 只用于明确的 FFI/COM 边界；每块写 `// SAFETY:`，说明别名、生命周期、释放者和线程 apartment。
- 音频热循环不得分配扩容、阻塞、写文件、访问 UI 或打日志。

## 5. 测试与工具

- 纯逻辑以观察行为和边界编单测；不写依赖真实声卡的测试。
- 音频库纯逻辑单测位于 `src/library/tests.rs`，由 `src/library.rs` 的 `#[cfg(test)] mod tests;` 引入；PCM/WAV/配置迁移等计划测试见 [`docs/refactor-plan.md`](docs/refactor-plan.md)。
- `scripts/check.cmd` 执行格式、`clippy -- -D warnings`、构建和行数门禁。
- `scripts/smoke.cmd` 执行设备/进程枚举及默认态、条目菜单 UI 快照。
- 真实路由、WAV 逐采样比对和桌面交互的验证方法见 [`DEBUGGING.md`](DEBUGGING.md)。

## 6. 文档同步

- 改动影响行为、模块/目录、线程状态、配置格式或排障路径时，同阶段更新相关文档。
  纯机械且不改变事实的格式调整无需单独增加文档。
- 源树/依赖/线程同步 `ARCHITECTURE.md`；UI 契约同步 `FRONTEND.md`；引擎/API 同步 `BACKEND.md` 与 `API.md`。
- 每个需求批次维护一份阶段实测文档，原则上至少记录 3 项相关可验收行为。
  后续 debug/bugfix/回归更新原文档，不按对话次数新建 Step；只有独立验收目标才新建。
  不足 3 项时只并入同批相邻需求，不拼无关内容；合并重复记录时保留有效事实/证据并清理索引。

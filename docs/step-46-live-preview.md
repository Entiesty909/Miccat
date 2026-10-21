# Step 46 — UI 开发预览与构建门禁工具收口

范围：同一批改善 Slint 开发反馈、Debug 构建可见性和代码规模门禁；不改变产品运行时音频行为。

功能要求：
1. `slint/live-preview` 只用于开发构建，保存 `.slint` 后进程内热重载；Rust/Cargo 逻辑变化仍重编并重启。
2. `build-debug.cmd` 固定仓库工作目录，失败时保留错误和非零退出码，成功打印 Debug exe 路径，避免资源管理器双击后窗口闪退。
3. 行数门禁采用常规目标/审查警告/硬阻断三级；保证大型装配文件可审查而不因常规目标直接阻断。

改动文件：
- `Cargo.toml`、`dev.ps1`、`scripts/dev.ps1`：Slint 1.18 官方 Live Preview 与开发重启规则。
- `scripts/build-debug.cmd`、`scripts/check-lines.ps1`：Debug 构建可见性、工作目录和 400/300、600/450、800/600 行数策略。
- `AGENTS.md`、`FILE-CONVENTIONS.md`、`PROJECT.md`、`ARCHITECTURE.md`、`FRONTEND.md`、`DEBUGGING.md`、`docs/refactor-plan.md`、`docs/step-02-rust-architecture.md`。

怎么跑：
- `./dev.ps1`：Debug Live Preview。
- `scripts\build-debug.cmd`：Debug 构建并显示产物路径。
- `scripts\check.cmd`：格式、Clippy、构建与行数门禁。
- `scripts\build-release.cmd`：确认发布构建不启用 Live Preview。

实测：2026-10-13 至 2026-10-14，Windows x64。
- Slint 1.18.0 Live Preview feature 构建、默认构建、开发脚本解析均通过；真实运行时同一 PID 的窗口标题热重载验证通过。
- `build-debug.cmd` 从默认工作目录双击路径执行成功；成功时打印 `target\debug\miccat.exe`，失败时保留错误码并暂停。
- 临时 801 行 Rust 探针触发行数硬门禁失败；常规超限只警告，临时探针已删除。
- 未改变发布 feature 集、业务接口或音频线程行为。

下一步：对 `.slint` 视觉迭代使用 `dev.ps1`；Rust/构建配置变化继续按脚本重编并重启。开发工具修补更新本记录。

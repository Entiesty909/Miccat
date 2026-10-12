# Step 61 — Slint 1.18 Live Preview

范围：
- 做了：将 `slint` 与 `slint-build` 对齐到 `~1.18`；以官方 `slint/live-preview` + `SLINT_LIVE_PREVIEW=1` 提供开发期进程内 UI 重载。
- 做了：根目录 `dev.ps1` 转发到 PowerShell 开发循环；默认构建并启动 Live Preview，Rust/Cargo 改动仍重编并重启，`.slint` 改动不触发脚本监听构建。
- 明确未做：修改音频引擎或 Windows/COM 逻辑；未在真实窗口中逐项点击按钮、开关和下拉框。

改动文件：
- `Cargo.toml`、`dev.ps1`、`scripts/dev.ps1`。
- `PROJECT.md`、`ARCHITECTURE.md`、`FRONTEND.md`、`DEBUGGING.md`、`docs/step-02-rust-architecture.md`。
- `docs/step-61-live-preview.md`。

官方依据：
- Slint 1.18 Live Preview 文档：<https://releases.slint.dev/1.18.0/docs/slint/guide/tooling/live-preview/>。
- Slint 1.18 Cargo feature 文档：<https://releases.slint.dev/1.18.0/docs/rust/slint/docs/cargo_features/>。
- 该方案要求编译 feature `slint/live-preview` 和编译环境变量 `SLINT_LIVE_PREVIEW=1`；运行时 interpreter 监视 `.slint` 文件。属性、模型、回调在 reload 时保留；改变 Rust 依赖的 Slint API 仍须重编/重启。

怎么跑：
- `./dev.ps1`：Debug Live Preview；保存 `.slint` 后当前窗口自动 reload。
- Rust 逻辑修改：开发脚本会检测 `src`、`build.rs`、`Cargo.toml`、`Cargo.lock`，重编后重启；也可手动运行 `scripts\build-dev.cmd build`，然后启动 `target\debug\miccat.exe`。
- 正式版本：`scripts\build-release.cmd`，不加入 `slint/live-preview`。

实测：2026-10-13，Windows x64。
- 依赖：`slint v1.18.0`、`slint-build v1.18.0`；`windows v0.62.2`、`windows-core v0.62.2`。`cargo tree -d` 显示多个 `windows-sys` 次版本（0.52.0、0.60.2、0.61.2），但 `windows` 与 `windows-core` 各自仅有一个 0.62.2 版本；未发现 COM facade 类型跨版本分裂。
- `SLINT_LIVE_PREVIEW=1 cargo check --features slint/live-preview`：通过。
- `SLINT_LIVE_PREVIEW=1 cargo build --features slint/live-preview`：通过。
- `cargo check`（默认、不含 Live Preview）：通过；`cargo tree -e features -i slint` 确认默认依赖 feature 未启用 `live-preview`。
- `SLINT_LIVE_PREVIEW=1 cargo run --features slint/live-preview -- --list-processes`：成功启动并输出实际进程列表。
- 根目录 `dev.ps1 -NoRun`：PowerShell 语法解析通过，真实执行首次构建并输出“编译成功，未启动”。
- 用 Live Preview 构建启动原生窗口后，将窗口标题临时改为 `Miccat HMR Probe`；同一 PID `11308` 的窗口标题随保存变更，未重启。标题已恢复为 `Miccat`；临时截图和快照产物已删除。
- `--snapshot-ui` 的路由、音频库、设置页均输出 1280×800 快照。
- 未手工点击真实窗口控件；Slint 对回调保留有官方保证，项目内回调绑定仍由原 `wire_callbacks` 注册，但按钮/开关/下拉框的真实点击结果未实测。实际运行根目录 `dev.ps1` 时，临时 Rust 注释变更与还原均触发重编并重启。

剩余限制：
- 若修改 Rust 代码直接访问的 Slint 属性或回调名称/签名，Live Preview 可能退出；需重编并重启。
- 本轮未手工操作按钮、开关、下拉框，亦未验证真实音频路由。

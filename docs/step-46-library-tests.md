# Step 46 — 拆分音频库单元测试

范围：将分类路径、移动/删除/重命名等测试从领域实现文件迁至测试子模块；不改变任何运行时逻辑。
改动文件：`src/library.rs`、`src/library/tests.rs`、`FILE-CONVENTIONS.md`、`ARCHITECTURE.md`、`docs/step-02-rust-architecture.md`、`PROJECT.md`。
怎么跑：`scripts\build-dev.cmd fmt -- --check`、`scripts\build-dev.cmd test --target-dir target-ui`、`scripts\build-dev.cmd clippy --target-dir target-ui -- -D warnings`、`scripts\build-dev.cmd build --target-dir target-ui`、`powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts\check-lines.ps1`。
实测：2026-09-29，Windows x64；18 项测试通过，严格 Clippy 与构建通过。`src/library.rs` 由 543 行降至 293 行，测试保留在 `src/library/tests.rs`。行数门禁通过；`src/audio/capture.rs`（449/400）和 `ui/app-window.slint`（383/300）按既有基线未增长。
下一步：按用户要求逐项核对路由页可点击控件是否接入有效领域逻辑。

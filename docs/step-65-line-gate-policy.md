# Step 65 — 行数门禁分级政策

范围：调整 `scripts/check-lines.ps1`，避免常规行数目标阻断职责清晰的实现，同时保留明显失控文件的硬阻断。未改变编译、Clippy、测试或产品运行行为。

改动文件：

- `scripts/check-lines.ps1`
- `AGENTS.md`
- `FILE-CONVENTIONS.md`
- `docs/refactor-plan.md`

新政策：

- `.rs` 400 行、`.slint` 300 行：常规维护目标。
- `.rs` 超过 600 行、`.slint` 超过 450 行：非阻断审查警告。
- `.rs` 超过 800 行、`.slint` 超过 600 行：门禁失败。
- 职责清晰的装配文件、桥接契约和 FFI 边界可以超过常规目标，但必须在阶段文档记录原因。
- `scripts/check.cmd` 的 fmt、Clippy 和 build 仍保持阻断；只放宽行数检查。

实测：

- `scripts\\check.cmd`：成功，完整门禁通过。
- 临时 801 行 Rust 探针：行数脚本返回失败，并报告 `hard limit 800`。
- 临时探针已删除，未进入工作树。

结论：合理的大文件不再因为 300/400 行常规目标阻断；超过 600/450 行仍会被提醒，超过 800/600 行仍会被硬拦截。
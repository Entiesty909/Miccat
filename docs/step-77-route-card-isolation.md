# Step 77 — 路由卡片独立与重复源校验
范围：让进程/音频文件路由卡片的增益、静音恢复、输出目标和 SFX 流按卡片槽位隔离；添加/替换前拒绝重复进程或音频。未改变麦克风单卡语义、全局总音量和路由启停入口。

改动文件：
- `src/config/mod.rs`：保存进程/音频卡片增益与卡片级输出 ID。
- `src/engine.rs`、`src/engine/output.rs`、`src/engine/fanout.rs`、`src/engine/volume.rs`、`src/audio/render.rs`、`src/audio/sfx.rs`：最多 4 个进程槽和 4 个音频槽独立原子增益、render 路由及并行 SFX 流。
- `src/engine/playback.rs`、`src/cli/mod.rs`、`src/audio/preview.rs`：适配带槽位的 SFX 命令与渲染参数。
- `src/ui_bridge/cache.rs`、`mod.rs`、`persist.rs`、`sources.rs`：维护并持久化卡片级派生状态。
- `src/ui_bridge/callbacks/route.rs`、`callbacks/route/sources.rs`、`callbacks/route_start.rs`、`callbacks/route_start/source_only.rs`、`callbacks/library_entries.rs`：卡片级操作接线和重复校验。
- `src/ui_bridge/callbacks/route/tests.rs`：增加重复进程/音频的添加与替换边界单测。
- `ARCHITECTURE.md`、`BACKEND.md`、`FRONTEND.md`、`PROJECT.md`：同步状态、引擎契约和验收记录。

怎么跑：
```bat
scripts\build-dev.cmd fmt
scripts\build-dev.cmd build
scripts\build-dev.cmd test
scripts\check.cmd
scripts\smoke.cmd
```

实测：2026-10-19，Windows x64。
- `scripts\build-dev.cmd fmt` 成功。
- `scripts\build-dev.cmd build` 成功。
- `scripts\build-dev.cmd test`：34 passed、0 failed。
- `scripts\check.cmd` 成功；仅有非阻断行数审查提示：`src/ui_bridge/callbacks/route/sources.rs` 650 行、`ui/app-window.slint` 471 行、`ui/components/add-source-panel.slint` 493 行。
- `scripts\smoke.cmd` 成功；覆盖设备/进程枚举、路由页、三种添加源窗口、重开窗口、源菜单和源电平快照。
- 未宣称真实桌面鼠标替换、真实多源听感和真实设备拔插已自动化验证；这些仍需 Windows 桌面/音频设备手工验收。

下一步：手工验证两张进程卡片分别调音量/静音/输出、两张音频卡片分别启停和调音量；验证把已有进程或音频替换到另一张卡片时不会生成重复源。

# Step 72 — 多源路由、替换入口与空闲性能
范围：修复路由源菜单删除、源菜单按钮保持可见；添加进程/音频源支持多项加入；进程与音频菜单增加替换入口并复用单Tab添加源对话框；文件加入路由不自动播放；优化声纹轮询空闲模型重建；麦克风下拉缩短并与源名称列对齐。

改动文件：

- `ui/components/source-actions-menu.slint`
- `ui/components/source-row.slint`
- `ui/components/add-source-dialog.slint`
- `ui/pages/route.slint`
- `src/ui_bridge/cache.rs`
- `src/ui_bridge/sources.rs`
- `src/ui_bridge/callbacks/route.rs`
- `src/ui_bridge/callbacks/route/sources.rs`
- `src/ui_bridge/callbacks/library_entries.rs`
- `src/ui_bridge/callbacks/route_start.rs`
- `src/engine/playback.rs`
- `src/ui_bridge/status.rs`
- `src/ui_bridge/library.rs`
- `src/ui_bridge/dev/snapshot.rs`
- `src/cli/mod.rs`
- `scripts/smoke.cmd`
- `ARCHITECTURE.md`、`BACKEND.md`、`FRONTEND.md`、`DEBUGGING.md`、`API.md`、`PROJECT.md`

实现：

- `...` 按钮始终可见；菜单打开后不再隐藏触发按钮。
- 删除菜单项执行后立即刷新源模型；文件源按卡片槽位删除，保留其它音频源，不再整组隐藏。
- 添加音频到路由只加入禁用卡片，不自动播放；用户单独启用后，运行中的路由才追加解码流。
- `route_audio_entries` / `route_audio_enabled` 保存多个音频源；启动路由时所有启用音频通过 `play_file_source()` 追加到 SFX mixer，不调用旧的单流 `play_file()`。
- 多进程继续使用 `selected_pids`；运行中的路由添加进程后按启用状态重建进程捕获槽位。
- 进程源菜单“替换进程”、音频源菜单“替换音频”打开单Tab添加源界面；替换不会显示其它两种类型卡。
- 麦克风下拉固定 190px，并在名称列内左对齐。
- 状态轮询只有峰值或历史实际变化时才重建声纹模型，空闲时跳过 `VecModel` 重建，降低空闲 CPU 分配和 UI 重绘。

验证：

```bat
scripts\build-dev.cmd fmt
scripts\check.cmd
scripts\build-dev.cmd test
scripts\smoke.cmd
```

实测：2026-10-15，Windows x64。

- Debug 构建通过。
- 26 个单元测试通过。
- 完整门禁通过；仅有既有非阻断行数审查提示：`ui/app-window.slint` 469 行、`ui/components/add-source-dialog.slint` 532 行。
- `scripts/smoke.cmd` 通过，覆盖源类型卡点击和源菜单弹层快照。
- 独立启动 UI 空闲测量：进程运行 2 秒累计 CPU 时间约 0.4375 秒，运行至 4 秒约 0.453125 秒；后 2 秒增量约 0.015625 秒，约 0.78% 单核时间。该数值是本机一次空闲快照测量，不等同所有设备/路由负载。
- 真实多进程捕获、多音频听感、替换对话框物理点击、删除菜单物理点击和设备路由仍需桌面/音频设备验收；未伪造这些结果。

下一步：桌面验证多个音频同时启用后的实际听感、替换入口提交、删除单卡和多进程动态热切换。

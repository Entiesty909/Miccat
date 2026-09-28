# Step 40 — 音频分类树可从空状态重建

范围：
- 做了：允许右键删除“全部”和“未分类”；“全部”只清除软件内音频条目，“未分类”只清除该分类及后代条目，不删除磁盘文件。
- 做了：分类树为空时，右键分类卡片空白区打开菜单；菜单显示“新建分类”，创建顶层分类并选中。
- 做了：右键普通分类创建其子分类；同级分类通过右键共同父分类创建，空白区创建顶层分类。
- 做了：分类卡片宽度维持 250 px；空白命中区只增加纵向伸展。
- 做了：导入音频归入当前选中分类；无选择时归入“未分类”。配置版本 3 记住已删除的根节点，旧版本迁移时补入默认根节点。
- 做了：分类拖拽支持拖入目标成为子分类、拖到上/下边缘成为同级前/后项；移动整棵子树及音频归属，并阻止循环和名称冲突。
- 做了：拖拽后重新展开路径，右键删除使用新路径；补充拖后删除回归测试。
- 做了：新建默认名称统一为“未命名分类”；右键重命名改为菜单内编辑确认，并同步整棵子树和音频条目路径。
- 未做：真实窗口鼠标拖拽与音频设备路由验收。

改动文件：
- `src/config/mod.rs`、`src/config/store.rs`、`src/library.rs`。
- `src/ui_bridge/mod.rs`、`src/ui_bridge/callbacks/library.rs`、`src/ui_bridge/callbacks/library_entries.rs`。
- `src/ui_bridge/dev/snapshot.rs`、`src/cli/mod.rs`、`ui/pages/player.slint`、`ui/overlays/context-menu.slint`。
- `scripts/smoke.cmd`、`API.md`、`FRONTEND.md`、`DEBUGGING.md`、`PROJECT.md`。

怎么跑：
- `cmd.exe /c "scripts\build-dev.cmd test"`：分类删除、空树新建、父子/兄弟路径、拖拽移动、拖后删除、重命名、导入归类和配置迁移测试通过。
- `cmd.exe /c "scripts\build-dev.cmd build --target-dir target\verify"`：独立验证目录构建成功。
- `target\verify\debug\mintcat-proc-mic.exe --snapshot-ui --category-menu ...` 与 `--category-row-menu`：分类右键事件快照成功。

实测：
- 2026-09-28，Windows x64。
- 单测：17 passed，0 failed。
- 构建：独立 `target\verify` 目录成功；默认 `target\debug` 因运行中的旧 exe 被 Windows 锁定，未覆盖。
- 冒烟：分类菜单和拖拽后分类行右键模拟均打开菜单。
- 快照验证 Slint 事件路径，不模拟真实桌面鼠标拖拽。

- 关闭旧 exe 后用默认 `target\debug` 构建；手工验证新建未命名分类、重命名确认、整枝拖到另一根树枝以及拖后删除。

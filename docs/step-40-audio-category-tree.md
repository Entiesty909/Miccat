# Step 40 — 音频分类树可从空状态重建

范围：
- 做了：允许右键删除“全部”和“未分类”；“全部”只清除软件内音频条目，“未分类”只清除该分类及后代条目，不删除磁盘文件。
- 做了：分类树为空时，右键分类卡片空白区打开菜单；菜单显示“新建分类”，创建顶层分类并选中。
- 做了：右键普通分类创建其子分类；同级分类通过右键共同父分类创建，空白区创建顶层分类。
- 做了：分类卡片宽度维持 250 px；空白命中区只增加纵向伸展。
- 做了：导入音频归入当前选中分类；无选择时归入“未分类”。配置版本 3 记住已删除的根节点，旧配置迁移时补入默认根节点。
- 未做：真实窗口鼠标操作和音频设备路由验收。

改动文件：
- `src/config/mod.rs`、`src/config/store.rs`、`src/library.rs`。
- `src/ui_bridge/mod.rs`、`src/ui_bridge/callbacks/library.rs`、`src/ui_bridge/callbacks/library_entries.rs`。
- `src/ui_bridge/dev/snapshot.rs`、`src/cli/mod.rs`、`ui/pages/player.slint`、`ui/overlays/context-menu.slint`。
- `scripts/smoke.cmd`、`API.md`、`FRONTEND.md`、`DEBUGGING.md`、`PROJECT.md`。

怎么跑：
- `cmd.exe /c "scripts\build-dev.cmd test"`：分类删除、空树新建、父子/兄弟路径、导入归类和配置迁移测试通过。
- `cmd.exe /c "scripts\build-dev.cmd build"`：开发版构建成功。
- `cmd.exe /c "scripts\smoke.cmd"`：设备/进程枚举成功，生成默认态、条目菜单、空分类树菜单三张 1080×560 RGBA 快照。

实测：
- 2026-09-28，Windows x64。
- 单测：10 passed，0 failed。
- 构建：成功。
- 冒烟：成功；空分类菜单快照显示“新建分类”，重命名和删除禁用。
- `--category-menu` 快照是程序设置菜单打开状态，只证明渲染，不模拟鼠标右键；真实空白区右键仍需在桌面交互确认。

下一步：
- 空分类树状态下右键分类卡片空白区域，选择“新建分类”，输入名称并添加音频，确认分类和条目持久化。

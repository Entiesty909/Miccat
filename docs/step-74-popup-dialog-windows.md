范围：将路由页添加/替换音频源和音频库分类/音频编辑从主窗口内嵌 Rectangle 覆盖层改为无标题栏的独立 Slint `PopupWindow` 模态悬浮层；不创建可见的额外原生顶层窗口，保持原有 Rust callback、配置保存和数据模型契约。

改动文件：

- `ui/pages/route.slint`
- `ui/pages/player.slint`
- `ui/components/add-source-dialog.slint`
- `ui/components/audio-library-edit-dialog.slint`
- `PROJECT.md`、`FRONTEND.md`

实现：

 - 添加/替换音频源使用 720×620 无标题栏 PopupWindow，普通模式显示统一三张类型卡，替换模式只显示目标单Tab。
 - 分类/音频编辑使用 560×460 无标题栏 PopupWindow，内容直接填充窗口，不再套一层旧的“遮罩 + 内层 dialog”视觉。
 - PopupWindow 使用 `no-auto-close`，由关闭按钮、取消 callback 或提交 callback 显式关闭；浮层持有交互焦点，主窗口不能抢到操作。
 - 路由源 `...` 菜单继续使用独立 PopupWindow；所有浮层不依赖主窗口内嵌遮罩层命中。

验证：

```bat
scripts\build-dev.cmd fmt
scripts\check.cmd
scripts\build-dev.cmd test
miccat.exe --snapshot-ui --page route --source-dialog-tab process --out popup-source.raw
scripts\smoke.cmd
```

实测：2026-10-15，Windows x64。

- Debug 构建、完整门禁和 26 个单元测试通过。
- 添加源独立窗口快照成功生成 1280×800，窗口位于主界面上层，进程列表和底部按钮可见可操作。
- 源菜单和三种添加源 Tab 的冒烟快照通过。
- 原生桌面点击编辑分类/音频、替换提交和关闭焦点仍需人工验收；未声称已完成物理鼠标验收。

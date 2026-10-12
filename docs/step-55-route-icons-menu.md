# Step 55 — Miccat 路由主页删减、单源菜单与真实图标
范围：删除确认的路由页冗余说明块和品牌状态行，保留路由/音频库/设置页签；启用开关移到源名称旁，音频文件暂停与停止保持左右并列；源操作菜单状态提升到路由页，同一时间只显示一个菜单。进程卡片优先显示 exe 图标，音频卡片优先显示条目 `icon_path` 指定的 Windows 图标，否则显示音频文件关联图标，失败回退现有默认图标。未改变音频路由动作和设备选择语义。

改动文件：
- `ui/components/app-header.slint`、`ui/pages/route.slint`：删除冗余品牌/说明内容，保留页签和当前音源区。
- `ui/components/source-row.slint`：启用开关靠近源名称，暂停/停止/更多横向并列，显示真实图片并保留默认图标回退。
- `ui/pages/route.slint`：以 `source-menu-index` 统一管理源菜单，只允许一个菜单显示。
- `src/platform/icon.rs`、`src/platform/mod.rs`：通过 Windows Shell/GDI 提取 exe 和文件图标并转换为 Slint RGBA 图片。
- `src/ui_bridge/sources.rs`、`src/ui_bridge/dev/snapshot.rs` 及对应 Slint 属性：贯通源图片模型。
- `src/config/mod.rs`：增加可选 `AudioEntry.icon_path`，兼容已有配置；当前仓库没有设置该字段的图形化入口。
- `Cargo.toml`：开启 Windows GDI、Storage FileSystem API features，无新增 crate。
- `FRONTEND.md`、`ARCHITECTURE.md`、`PROJECT.md`：同步 UI 契约、平台模块和阶段索引。

怎么跑：
```bat
scripts\build-dev.cmd fmt -- --check
scripts\build-dev.cmd build
scripts\smoke.cmd
target\debug\miccat.exe --snapshot-ui --page route --out target\route-icons-check.raw
```

实测：2026-10-11，Windows x64。
- 路由软件渲染快照成功输出 1280x800；页签保留，说明块和品牌状态行删除；进程行显示实际 exe 图标，音频行显示 Windows 关联文件图标；启用开关靠近源名称，音频暂停/停止并列。
- 同一源菜单状态由页面索引控制，切换源菜单时旧菜单条件隐藏；菜单打开时对应 `...` 不渲染。
- 真实桌面点击菜单切换、图形化自定义音频图标设置和多窗口宽度仍需手工验收。

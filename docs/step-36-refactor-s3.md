# Step 36 — Slint 窗口与右键菜单拆分

范围：
- 做了：`ui/main.slint` 重命名为 `ui/app-window.slint`，构建入口同步更新。
- 做了：将分类/音频右键菜单覆盖层迁移到 `ui/overlays/context-menu.slint`。
- 做了：主窗口只负责属性、页面装配、状态栏和覆盖层回调转发；菜单自身负责遮罩、坐标钳制和菜单动作。
- 明确未做：页面重复控件抽取、ListView/ScrollView 虚拟化、主题 token 全量收口。

改动文件：
- `ui/app-window.slint`、`ui/overlays/context-menu.slint`、`build.rs`。
- `docs/step-36-refactor-s3.md`。

怎么跑：
- `cmd.exe /c "scripts\build-dev.cmd build"`
- `cmd.exe /c "scripts\smoke.cmd"`

实测：
- Slint 编译和 Rust 构建通过。
- 设备/进程枚举通过。
- 默认状态与音频条目菜单快照均输出 1080×560 RGBA。
- 未执行 Windows 桌面手工右键交互验收。

下一步：
- S4 拆分引擎和音频原语文件，收敛重复 PCM、重采样、运行时与错误逻辑。

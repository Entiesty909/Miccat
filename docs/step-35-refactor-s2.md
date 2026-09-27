# Step 35 — 音频领域门面

范围：
- 做了：新增 `src/engine/monitor.rs`，试听输出由 `AudioEngine` 管理，UI 只调用引擎 API。
- 做了：新增 `src/engine/playback.rs`，文件解码、SFX/试听流分发、取消、停止与播放状态由引擎管理。
- 做了：新增根领域模块 `src/library.rs`，集中分类路径、递归重命名/删除、可见索引和文件验证规则，并加入单测。
- 做了：移除 `ui_bridge/` 对 `audio::decode`、`audio::sfx`、`audio::preview` 的直接引用。
- 明确未做：S4 的音频原语拆分与实时路径分配收敛；CLI `--capture`/`--route` 仍保留旧入口。

改动文件：
- `src/engine.rs`、`src/engine/monitor.rs`、`src/engine/playback.rs`。
- `src/library.rs`。
- `src/audio/preview.rs`（增加非阻塞试听停止请求）。
- `src/ui_bridge/` 播放、状态、设置、分类回调和缓存模块。
- `docs/step-35-refactor-s2.md`。

怎么跑：
- `cmd.exe /c "scripts\build-dev.cmd build"`
- `cmd.exe /c "scripts\build-dev.cmd test"`
- `cmd.exe /c "scripts\smoke.cmd"`
- `git grep -n -E 'audio::(decode|sfx|preview)|PreviewEngine|sfx_sender|stream_to_channel' -- src/ui_bridge`

实测：
- 构建通过；Clippy 默认级别告警保留，未阻断。
- 音频库领域测试通过：2 passed。
- 设备/进程枚举与两个 1080×560 UI 快照冒烟通过。
- 未执行真实桌面双击、拖动进度、试听听感和设备路由人工验收。

下一步：
- 提交 S2 中文 commit。
- S3 拆分 Slint 主窗口、右键菜单覆盖层和重复组件。

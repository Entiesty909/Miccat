# Step 48 — 音频库管理与路由文件播放收口

范围：同一批收口音频库分类/条目管理、三栏布局与编辑、路由增益/峰值显示、播放设置位置及路由文件手动播放语义。真实数据和配置仍是权威；不改磁盘文件名，不伪装物理麦克风为 render 目标。

功能要求：
1. 分类/列表/详情三栏可调；列表只展示当前分类及后代，搜索/排序/分页索引一致；分类与条目名称/图标变更保存到 AppConfig。
2. 分类重命名/删除保持层级路径与图标映射；自定义图标、首字和默认音符使用既有原生文件选择/字体，不加入示例数据或依赖。
3. 播放设置归路由卡片：详情不再提供输出/文件音量/本机试听控制；清理旧 `file_output_id` 引起的未使用虚拟端点，无卡片级输出时跟随主输出。
4. 启用文件源、启动全局路由或单独转发均不自动播放；用户点击卡片播放，禁用时停止该槽，重新启用不续播，状态与音频库全局播放器隔离。
5. 文件停止/未播放显示播放三角，播放显示暂停；真实峰值来源于 capture/SFX 采样，UI 不用音量值伪造 dB；总线增益 dB 与信号峰值 dBFS 分开。

改动文件：
- `ui/pages/player.slint`、`ui/pages/route.slint`、`ui/components/{category-sidebar,library-splitter,audio-detail,audio-list,audio-transport,source-row,add-source-panel}.slint`、`ui/components/page-host.slint`、`ui/windows.slint`。
- `src/library.rs`、`src/library/tests.rs`：纯分类/路径/移动/删除逻辑测试与领域实现分离，避免单体文件膨胀。
- `src/engine.rs`、`src/engine/{playback,output}.rs`、`src/audio/sfx.rs`：按文件槽位播放/暂停/停止。
- `FRONTEND.md`、`ARCHITECTURE.md`、`BACKEND.md`、`API.md`、`DEBUGGING.md`、`PROJECT.md`。
- `FILE-CONVENTIONS.md`、`docs/step-02-rust-architecture.md`：登记领域测试子模块和文件边界。

怎么跑：
- `scripts\build-dev.cmd fmt -- --check`
- `scripts\build-dev.cmd test`
- `scripts\check.cmd`
- `scripts\smoke.cmd`
- `target\debug\miccat.exe --snapshot-ui --page player --out "%TEMP%\miccat-player.raw"`
- `target\debug\miccat.exe --snapshot-ui --page route --out "%TEMP%\miccat-route.raw"`
- `target\debug\miccat.exe --probe-playback <file> --seconds 3 --out "%TEMP%\probe.wav"`

实测：2026-10-21，Windows x64。
- 最终 `scripts\check.cmd`、Smoke、38 项单元测试和 Debug 构建通过；播放器/路由软件快照生成成功。
- 较早的视觉/分类修正轮次曾分别跑 23、34 项测试；最新路由播放语义测试覆盖停止/暂停槽位边界。
- 该需求批次早期将分类/路径/移动/删除测试迁到 `src/library/tests.rs`：18 项测试通过，`src/library.rs` 从 543 行降至 293 行；后续音频库功能在同一测试模块持续回归。
- `--list-devices` 列出物理播放端点及 VB-CABLE、Steam 等虚拟端点；`--probe-playback` 是离线采样探针，不替代真实设备听感。
- 未声称完成真实 render 听感、麦克风默认设备切换或原生鼠标编辑验收。

下一步：在真实设备确认目标输出与文件手动播放/暂停/停止；同批音频库和文件路由修复更新本记录，不另建 Step。

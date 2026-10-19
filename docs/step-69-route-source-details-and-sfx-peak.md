# Step 69 — 音频源详情与实时峰值

范围：添加源对话框的音频页显示真实音频条目及其格式/估算码率、时长、大小、分类、快捷键和循环状态；SFX 混音器采集真实峰值，路由源行读取该峰值；空分类树提供可命中的右键区域。未写入示例音频、快捷键或设备数据，未测试真实路由声音。

改动文件：

- `ui/components/add-source-dialog.slint`
- `ui/components/audio-source-list.slint`
- `ui/components/audio-source-details.slint`
- `ui/components/category-sidebar.slint`
- `ui/components/page-host.slint`
- `ui/pages/route.slint`
- `ui/app-window.slint`
- `src/ui_bridge/library.rs`
- `src/ui_bridge/sources.rs`
- `src/ui_bridge/dev/snapshot.rs`
- `src/ui_bridge/callbacks/library.rs`
- `src/engine.rs`、`src/engine/output.rs`、`src/engine/fanout.rs`、`src/engine/status.rs`
- `src/audio/render.rs`、`src/audio/sfx.rs`、`src/audio/preview.rs`
- `src/platform/icon.rs`、`src/cli/mod.rs`
- `Cargo.toml`、`Cargo.lock`
- `scripts/smoke.cmd`、`FRONTEND.md`、`BACKEND.md`、`DEBUGGING.md`、`PROJECT.md`

实现说明：

- 路由音频目录模型直接从 `AppConfig.audio_entries` 派生，名称、分类、快捷键和循环标记没有静态案例数据。
- `AudioSourceList` 与 `AudioSourceDetails` 分担列表和详情职责；`add-source-dialog.slint` 收回到 419 行。
- SFX mixer 将块峰值写入 `AtomicU32`，UI 沿用 50 ms 状态轮询读取，不从音量值伪造峰值。
- 文件图标只对识别的图像扩展名调用 Slint 解码器；音频/可执行文件仍交给 Windows Shell 提取图标。
- 增加 `--source-audio-dialog` 快照模式和 smoke 覆盖；快照使用当前配置、进程和设备模型。
- 修正 Slint 1.18 图片 feature 名称为 `image-default-formats`，保留自定义图片图标解码能力；未新增直接 Cargo 依赖。

怎么跑：

```bat
scripts\build-dev.cmd fmt
scripts\check.cmd
scripts\build-dev.cmd test
scripts\smoke.cmd
miccat.exe --snapshot-ui --page route --source-audio-dialog --out "%TEMP%\miccat-source-audio.raw"
```

实测：2026-10-15，Windows x64；完整门禁通过；23 个单元测试通过；smoke 枚举设备/进程并生成 5 张 1280×800 快照，其中包括空分类区、分类行和音频源详情；音频源面板使用本机当前配置条目完成视觉核对。`ui/app-window.slint` 为 465 行，超过 450 行审查提示线但未超过硬行数门禁；其余本轮新增/拆分的 Slint 组件低于 450 行。真实路由播放仍未实测。

下一步：继续按 `docs/audio-library-redesign-plan.md` 的切片重构音频库主页面；本阶段未改变该页面的三栏布局契约。

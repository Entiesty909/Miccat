# Step 64 — 音频库 tab 目标布局切片

范围：按用户最新参考图调整音频库 tab 内容区。重点修正左侧快捷键区域和右侧详情设置区域；顶部 header、页签、路由页和设置页未修改。设计稿中的具体音频名、快捷键、路径、分类数量和设备名没有写入代码或测试数据。

改动文件：

- `ui/pages/player.slint`
- `ui/components/audio-list.slint`
- `ui/components/audio-detail.slint`
- `ui/components/category-sidebar.slint`
- `ui/components/page-host.slint`
- `ui/app-window.slint`
- `src/ui_bridge/cache.rs`
- `src/ui_bridge/library.rs`
- `src/ui_bridge/callbacks/library_view.rs`
- `src/ui_bridge/callbacks/player.rs`
- `FRONTEND.md`
- `API.md`
- `docs/audio-library-redesign-plan.md`

当前行为：

- 左侧快捷键区域继续读取真实 `AppConfig.audio_entries` 和路由快捷键配置，按设计稿使用简单列表行展示。
- 列表工具栏恢复为搜索、添加音频和按名称排序；列表列为名称、分类、时长、快捷键、状态和操作。
- 音频名称下方显示真实格式、采样率和码率摘要；不写入设计稿示例格式数据。
- 详情区按参考图显示播放按钮、本机试听、输出设备、文件音量、快捷键输入、循环播放、分类、本机试听、文件信息和路径。
- 快捷键字段支持真实绑定和清除，删除音频调用现有配置删除逻辑，分类按钮复用现有分类修改逻辑。
- 详情音量复用文件源 SFX 增益，调用 `AudioEngine::set_sfx_volume()` 并防抖保存 `AppConfig.mix.sfx`。

怎么跑：

```bat
scripts\\build-dev.cmd fmt
scripts\\build-dev.cmd build
scripts\\build-dev.cmd test
target\\debug\\miccat.exe --snapshot-ui --page player --out "%TEMP%\\miccat-audio-library.raw"
scripts\\check.cmd
```

实测：Windows x64。

- `scripts\\build-dev.cmd fmt`：成功。
- `scripts\\build-dev.cmd build`：成功。
- `scripts\\build-dev.cmd test`：成功，23 个测试通过。
- `--snapshot-ui --page player`：成功生成 1280×800 真实配置快照，并已展示用于和参考图对比。
- `scripts\\check.cmd`：成功；行数检查采用新的分级政策，审查阈值只警告，硬上限才失败。

下一步：只做用户继续指出的像素级间距、宽度、字号和控件对齐，不再加入参考图中没有的波形、格式筛选标签或底部额外操作。

# Step 73 — 麦克风转发卡死修复
范围：修复启用物理麦克风后停止路由/切换麦克风导致 UI 卡死。未改变麦克风捕获语义、设备选择语义或默认麦策略。

根因：

`AudioEngine::set_mic_source()` 创建了 `stop` 原子标志并把其 clone 传入麦克风捕获线程，但此前没有把原始 `stop` 保存到 `SourceThreads.mic_stop`。`stop_mic_source()` 会取出线程句柄并直接 `join()`；由于停止标志为空，捕获线程一直在 `WaitForSingleObject(event, 80)` 循环中运行，UI 线程永久等待，表现为“转发麦克风后整个软件卡死”。

修复：

- 在 `set_mic_source()` 保存 `s.mic_stop = Some(stop)`。
- 现有 `stop_mic_source()` 能先设置原子停止标志，再 join 捕获线程；捕获循环最多等待一个 80ms 事件超时后退出。
- 运行中替换麦克风和停止路由共用同一安全关停路径。

改动文件：

- `src/engine.rs`
- `docs/step-73-microphone-forwarding-deadlock.md`

验证：

```bat
scripts\build-dev.cmd fmt
scripts\check.cmd
scripts\build-dev.cmd test
scripts\smoke.cmd
```

实测：2026-10-15，Windows x64。

- Debug 构建通过。
- 完整门禁通过；行数提示为既有非阻断审查提示。
- 26 个单元测试通过。
- UI/设备冒烟通过，覆盖源页面、类型卡点击和源菜单快照。
- 静态等待链已核对：`stop_mic_source()` 设置 `mic_stop` 后 join；捕获循环检查同一停止标志并以 80ms 超时退出。
- 当前工具链未执行真实物理麦克风→启动→停止的桌面听感路径；该路径仍需 Windows 设备手工验收。修复结果依据代码等待链和构建/测试/冒烟证据，不冒称真实设备路径已完成。

下一步：手工验证选择物理麦克风、开始转发、停止转发、运行中切换麦克风，以及设备拔出后的恢复。

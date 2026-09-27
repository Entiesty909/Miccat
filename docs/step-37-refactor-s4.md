# Step 37 — 引擎与音频职责拆分

范围：
- 做了：将多输出 fan-out、格式转换和 render 线程工厂迁移到 `src/engine/fanout.rs`。
- 做了：将 CLI Process Loopback → WAV 验证路径迁移到 `src/audio/dump.rs`。
- 做了：保留音频线程、ring、格式和错误行为，仅移动职责与收紧可见性。
- 明确未做：PCM/重采样/运行时公共原语的进一步合并；实时分配收敛留待后续 S4 子步骤。

改动文件：
- `src/engine.rs`、`src/engine/fanout.rs`。
- `src/audio/capture.rs`、`src/audio/dump.rs`、`src/audio/mod.rs`、`src/main.rs`。
- `docs/step-37-refactor-s4.md`。

怎么跑：
- `cmd.exe /c "scripts\build-dev.cmd build"`
- `cmd.exe /c "scripts\build-dev.cmd test"`
- `cmd.exe /c "scripts\smoke.cmd"`

实测：
- 构建通过。
- 音频库领域单测通过：2 passed。
- 设备/进程枚举和 UI 快照冒烟通过。
- 未执行真实 `--capture` WAV 采样比对和真实 `--route` 听感验收。

下一步：
- S5 拆分 CLI、platform 基础设施和 config 存储。

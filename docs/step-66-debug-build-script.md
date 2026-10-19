# Step 66 — Debug 构建脚本可见性

问题：双击 `scripts\\build-debug.cmd` 时窗口快速关闭，构建失败原因不可见；从资源管理器启动时工作目录也不应依赖当前 shell。

改动：

- `scripts\\build-debug.cmd` 先切换到仓库根目录，再调用 `build-dev.cmd build`。
- 构建失败时显示错误并暂停，保留非零退出码。
- 构建成功时打印明确的 Debug exe 路径并暂停，双击启动不会闪退。
- Debug exe 仍为 `target\\debug\\miccat.exe`。

实测：

```bat
scripts\\build-debug.cmd
```

命令行构建成功，输出：

```text
[info] workspace: D:\code\MintcatProcMic
[ok] Debug binary: D:\code\MintcatProcMic\target\debug\miccat.exe
```

Cargo 显示 `Finished dev profile` 且耗时很短时，代表增量缓存认为源码没有变化，不会重新链接 exe；这不等于脚本没有执行。修改源码或构建配置后会重新编译并更新目标文件。

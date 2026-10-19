# Step 67 — 音频库五项交互改造

范围：

1. 音频库分类栏、列表栏、详情栏增加可拖动宽度分隔条。
2. 中间列表只展示当前分类及其后代分类的条目；“全部”展示全库。
3. 移除列表中不属于参考图的蓝色选中竖线。
4. 分类右键“重命名”改为“编辑”，打开现代化分类编辑对话框。
5. 音频条目右键“编辑”打开名称和图标编辑对话框；名称只影响软件显示，不改磁盘文件名；默认 glyph 使用 Windows Segoe MDL2 Assets `MusicNote` U+EC4F。

数据与持久化：

- 分类图标保存在 `AppConfig.audio_category_icons`。
- 音频条目图标使用 `AudioEntry.icon_glyph` 或 `icon_path`。
- 分类重命名会迁移分类图标路径；删除分类会删除对应图标映射。
- 自定义图标通过原生文件选择器选择，不引入图片/网络依赖。
- 音频条目名称、图标 glyph 和图标路径均走 `SaveQueue` 防抖保存。

怎么跑：

```bat
scripts\\build-dev.cmd fmt
scripts\\build-dev.cmd build
scripts\\build-dev.cmd test
scripts\\check.cmd
target\\debug\\miccat.exe --snapshot-ui --page player --out "%TEMP%\\miccat-five-items.raw"
```

实测：Windows x64。

- Debug 构建：成功。
- 单元测试：成功，23 个测试通过。
- 完整门禁：成功；行数审查警告非阻断，硬上限通过。
- 音频库快照：成功生成 1280×800 真实配置截图。
- Microsoft Learn 资料确认 `Segoe MDL2 Assets` 的 `MusicNote` glyph 为 U+EC4F；使用 Windows 自带字体，不新增依赖。

未覆盖：原生桌面鼠标拖动分隔条、分类/条目编辑对话框的人工点击路径仍需 Windows 窗口手工验收；编译和软件渲染快照已通过。

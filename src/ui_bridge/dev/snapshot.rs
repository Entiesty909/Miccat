//! 使用 Slint 软件渲染器输出音频库 UI 快照。

use super::super::*;

/// 临时：把主窗口按音频库布局渲染成原始 RGBA，用于核对菜单与索引列。
pub(crate) fn snapshot_to_file(path: &str, with_entry_menu: bool) -> Result<(), Error> {
    let ui = MainWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?;
    let persisted = config::load();
    let cache = Rc::new(RefCell::new(UiCache {
        audio_categories: persisted.audio_categories.clone(),
        audio_entries: persisted.audio_entries.clone(),
        audio_category: "全部".into(),
        ..UiCache::default()
    }));
    ui.set_page(1);
    // 走真实列表填充路径，用本机配置里的音频条目。
    update_audio_list(&ui, &cache);
    ui.set_audio_entry_index(0);
    ui.set_audio_current_name(slint::SharedString::from("豪大大鸡排原版"));
    ui.set_audio_current_position(slint::SharedString::from("0:47 / 3:55"));
    ui.set_player_playing(true);
    ui.set_audio_progress(0.2);
    if with_entry_menu {
        ui.set_audio_entry_index(1);
        ui.set_audio_entry_menu_x(420.0);
        ui.set_audio_entry_menu_y(150.0);
        ui.set_audio_entry_menu_open(true);
    }
    ui.window().set_size(slint::LogicalSize::new(1080.0, 560.0));
    ui.window()
        .show()
        .map_err(|e| Error::InvalidArgs(e.to_string()))?;
    // 软件渲染器按需渲染：先压一次事件循环，再取快照。
    let _ = slint::invoke_from_event_loop(|| {});
    let snapshot = ui
        .window()
        .take_snapshot()
        .map_err(|e| Error::InvalidArgs(e.to_string()))?;
    let mut bytes = Vec::with_capacity(8 + snapshot.as_bytes().len());
    bytes.extend_from_slice(&snapshot.width().to_le_bytes());
    bytes.extend_from_slice(&snapshot.height().to_le_bytes());
    bytes.extend_from_slice(snapshot.as_bytes());
    std::fs::write(path, &bytes)?;
    println!(
        "snapshot {}x{} -> {path}",
        snapshot.width(),
        snapshot.height()
    );
    Ok(())
}

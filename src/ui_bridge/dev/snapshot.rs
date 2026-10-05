//! 使用 Slint 软件渲染器输出音频库 UI 快照。

use super::super::*;

/// 快照渲染选项：页面、菜单模拟与诊断面板状态。
#[derive(Clone, Copy, Default)]
pub(crate) struct SnapshotOptions<'a> {
    /// 输出文件路径。
    pub(crate) path: &'a str,
    /// 页面名：route / player / settings。
    pub(crate) page: &'a str,
    /// 在音频库页打开条目右键菜单。
    pub(crate) entry_menu: bool,
    /// 在音频库页模拟空白分类区右键。
    pub(crate) empty_category_menu: bool,
    /// 在音频库页模拟分类行右键。
    pub(crate) category_row_menu: bool,
    /// 打开问题中心并填入示例条目。
    pub(crate) diagnostics: bool,
    /// 问题中心切到「日志」标签页。
    pub(crate) diagnostics_log: bool,
    /// 只显示状态栏问题徽标，不打开面板。
    pub(crate) diagnostics_badge: bool,
    /// 不填示例条目，直接渲染当前真实诊断快照。
    pub(crate) diagnostics_live: bool,
    /// 设置页分组下标；负值表示不改。
    pub(crate) settings_section: i32,
}

/// 把主窗口按指定页面渲染成原始 RGBA，用于核对布局与交互状态。
pub(crate) fn snapshot_to_file(options: SnapshotOptions<'_>) -> Result<(), Error> {
    let SnapshotOptions {
        path,
        page,
        entry_menu,
        empty_category_menu,
        category_row_menu,
        diagnostics,
        diagnostics_log,
        diagnostics_badge,
        diagnostics_live,
        settings_section,
    } = options;
    let with_entry_menu = entry_menu;
    let simulate_empty_category_right_click = empty_category_menu;
    let simulate_category_row_right_click = category_row_menu;
    let page_index = match page {
        "route" => 0,
        "player" => 1,
        "settings" => 2,
        _ => return Err(Error::InvalidArgs("unknown UI snapshot page".into())),
    };
    if page_index != 1
        && (with_entry_menu
            || simulate_empty_category_right_click
            || simulate_category_row_right_click)
    {
        return Err(Error::InvalidArgs(
            "menu snapshots require the player page".into(),
        ));
    }
    let ui = MainWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?;
    let persisted_state = Rc::new(RefCell::new(config::load()));
    if hydrate_audio_durations(&persisted_state) {
        config::save(&persisted_state.borrow());
    }
    let persisted = persisted_state.borrow().clone();
    let (categories, audio_entries, audio_category) = if simulate_empty_category_right_click {
        (Vec::new(), Vec::new(), String::new())
    } else if simulate_category_row_right_click {
        let categories = vec![
            "分类1".into(),
            "分类2".into(),
            "分类3".into(),
            "分类2/分类1".into(),
        ];
        let category = crate::library::category_paths(&categories, &[])[0].clone();
        (categories, Vec::new(), category)
    } else {
        let categories = persisted.audio_categories.clone();
        let entries = persisted.audio_entries.clone();
        let category = crate::library::category_paths(&categories, &entries)
            .first()
            .cloned()
            .unwrap_or_default();
        (categories, entries, category)
    };
    let cache = Rc::new(RefCell::new(UiCache {
        all_devices: persisted.output.all_devices,
        audio_categories: categories,
        audio_entries,
        audio_category,
        ..UiCache::default()
    }));
    ui.set_page(page_index);
    if page_index == 2 && settings_section >= 0 {
        ui.set_settings_section_index(settings_section);
    }
    ui.set_audio_categories(ModelRc::new(VecModel::from(audio_categories(&cache))));
    if page_index == 1 {
        refill_devices(&ui, &cache, false);
        let cache_state = cache.borrow();
        let global_output_index = persisted
            .output
            .device_id
            .as_deref()
            .and_then(|id| {
                cache_state
                    .dests
                    .iter()
                    .position(|device| device.render_id == id)
            })
            .unwrap_or(0);
        let file_output_id = persisted
            .output
            .file_output_id
            .as_deref()
            .or(persisted.output.device_id.as_deref());
        let file_output_index = file_output_id
            .and_then(|id| {
                cache_state
                    .dests
                    .iter()
                    .position(|device| device.render_id == id)
            })
            .unwrap_or(global_output_index);
        ui.set_device_index(global_output_index as i32);
        ui.set_file_output_index(file_output_index as i32);
        drop(cache_state);
        ui.set_preview_enabled(persisted.preview_enabled);
        ui.set_preview_volume(persisted.preview_volume * 100.0);
        update_audio_list(&ui, &cache);
        ui.set_audio_entry_index(0);
        ui.set_audio_current_name("".into());
        ui.set_audio_current_position("".into());
        ui.set_player_playing(false);
        ui.set_player_entry_index(-1);
        ui.set_audio_progress(0.0);
    } else {
        refill(&ui, &cache, true);
        restore_saved_selection(&ui, &cache, &persisted);
        ui.set_mix_mic(persisted.route.mix_mic);
        ui.set_master_volume(persisted.mix.master * 100.0);
        ui.set_preview_enabled(persisted.preview_enabled);
        ui.set_preview_volume(persisted.preview_volume * 100.0);
        if page_index == 0 {
            let cache = cache.borrow();
            let mut rows = super::super::sources::SourceRows::default();
            if let Some(process) = cache.processes.first() {
                rows.kinds.push(0);
                rows.icons.push("\u{E7F4}".into());
                rows.titles.push(process.name.clone().into());
                rows.subtitles.push("进程音频".into());
                rows.details.push(format!("PID: {}", process.pid).into());
                rows.statuses.push("可添加".into());
                rows.active.push(false);
                rows.volumes.push(persisted.mix.process * 100.0);
                rows.muted.push(false);
            }
            if let Some(entry) = cache.audio_entries.first() {
                super::super::sources::add_file_row(
                    &mut rows,
                    &entry.name,
                    "已导入",
                    false,
                    false,
                    persisted.mix.sfx,
                    0,
                );
                let detail = format!(
                    "{} · {}",
                    crate::library::category_leaf(&entry.category),
                    super::super::format::format_duration_hns(entry.duration_hns)
                );
                if let Some(slot) = rows.details.last_mut() {
                    *slot = detail.into();
                }
            }
            if let Some(mic_id) = persisted.output.mic_id.as_deref()
                && let Some(index) = cache.mic_ids.iter().position(|id| id == mic_id)
                && let Some(name) = cache.mic_names.get(index)
            {
                rows.kinds.push(2);
                rows.icons.push("\u{E720}".into());
                rows.titles.push(name.clone().into());
                rows.subtitles.push("麦克风音频".into());
                rows.details.push(name.clone().into());
                rows.statuses.push(if persisted.route.mix_mic {
                    "已启用".into()
                } else {
                    "未启用".into()
                });
                rows.active.push(persisted.route.mix_mic);
                rows.volumes.push(persisted.mix.mic * 100.0);
                rows.muted.push(false);
            }
            rows.apply(&ui);
        }
    }
    if with_entry_menu {
        ui.set_audio_entry_index(1);
        ui.set_audio_entry_menu_x(420.0);
        ui.set_audio_entry_menu_y(150.0);
        ui.set_audio_entry_menu_open(true);
    }
    if simulate_empty_category_right_click {
        ui.set_audio_category_index(-1);
    }
    if simulate_category_row_right_click {
        ui.set_audio_category_index(2);
    }
    if diagnostics || diagnostics_log || diagnostics_badge {
        seed_diagnostics(&ui, &persisted, diagnostics_log, diagnostics_badge);
    }
    if diagnostics_live {
        let last = std::cell::Cell::new(usize::MAX);
        diagnostics::sync(&ui, &last);
    }
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    ui.window()
        .show()
        .map_err(|e| Error::InvalidArgs(e.to_string()))?;
    if simulate_empty_category_right_click {
        let _ = ui
            .window()
            .take_snapshot()
            .map_err(|e| Error::InvalidArgs(e.to_string()))?;
        dispatch_right_click(&ui, slint::LogicalPosition::new(100.0, 300.0));
        if !ui.get_audio_category_menu_open() {
            return Err(Error::InvalidArgs(
                "empty category area did not open its right-click menu".into(),
            ));
        }
    }
    if simulate_category_row_right_click {
        let _ = ui
            .window()
            .take_snapshot()
            .map_err(|e| Error::InvalidArgs(e.to_string()))?;
        dispatch_right_click(&ui, slint::LogicalPosition::new(100.0, 256.0));
        if !ui.get_audio_category_menu_open() || ui.get_audio_category_index() != 2 {
            return Err(Error::InvalidArgs(
                "category row right-click did not open its matching menu".into(),
            ));
        }
    }
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

/// 往诊断中心塞入示例条目，用于渲染问题中心面板。
fn seed_diagnostics(
    ui: &MainWindow,
    persisted: &config::AppConfig,
    log_tab: bool,
    badge_only: bool,
) {
    let hub = crate::diagnostics::hub();
    hub.clear();
    hub.report(
        crate::diagnostics::Severity::Error,
        "audio.render",
        "无法连接到音频设备\n设备可能已经被拔出。",
        "E-DEV: render init: device not found (0x88890004)",
        "在设置页重新选择输出设备；必要时重新插拔耳机/线缆。",
    );
    hub.report(
        crate::diagnostics::Severity::Warn,
        "config",
        "配置解析失败，已使用默认值",
        "config parse failed; using defaults",
        "",
    );
    hub.report(
        crate::diagnostics::Severity::Warn,
        "app",
        "进程已退出",
        "",
        "",
    );
    diagnostics::init(ui, persisted);
    let last = std::cell::Cell::new(usize::MAX);
    diagnostics::sync(ui, &last);
    let state = ui.global::<DiagState>();
    state.set_open(!badge_only);
    state.set_selected(2);
    state.set_tab(log_tab as i32);
}

fn dispatch_right_click(ui: &MainWindow, position: slint::LogicalPosition) {
    let window = ui.window();
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    window.dispatch_event(slint::platform::WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Right,
    });
    window.dispatch_event(slint::platform::WindowEvent::PointerReleased {
        position,
        button: slint::platform::PointerEventButton::Right,
    });
}

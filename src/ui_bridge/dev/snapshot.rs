//! 构造开发期 UI 场景，模拟交互并写出 Slint 软件渲染快照。

use super::super::*;
use slint::Model;

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
    /// 路由页添加源初始页下标；-1 表示关闭，0/1/2 对应进程/音频/麦克风。
    pub(crate) source_dialog_tab: i32,
    /// 先打开其它类型，再通过鼠标点击目标类型并校验切换；-1 表示不模拟。
    pub(crate) source_dialog_click_tab: i32,
    /// 对同一源窗口执行隐藏/重开并比较像素，检查再次显示的完整重绘。
    pub(crate) source_dialog_reopen: bool,
    /// 打开问题中心并填入示例条目。
    pub(crate) diagnostics: bool,
    /// 在路由页点击第一条音源的更多按钮，生成 PopupWindow 快照。
    pub(crate) source_menu_click: bool,
    /// 在路由页展开实时源与总线电平 Popover。
    pub(crate) source_meter: bool,
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
    // 先解构选项，把命令行场景转换成一次性的内存 fixture；快照流程不会把模拟交互写回运行态。
    let SnapshotOptions {
        path,
        page,
        entry_menu,
        empty_category_menu,
        category_row_menu,
        source_dialog_tab,
        source_dialog_click_tab,
        source_dialog_reopen,
        source_menu_click,
        source_meter,
        diagnostics,
        diagnostics_log,
        diagnostics_badge,
        diagnostics_live,
        settings_section,
    } = options;
    // 这些别名保留场景语义，后面的校验和事件模拟可读且不会混淆真实配置字段。
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
    if !(-1..=2).contains(&source_dialog_tab) {
        return Err(Error::InvalidArgs("unknown source dialog tab".into()));
    }
    if source_dialog_tab >= 0 && page_index != 0 {
        return Err(Error::InvalidArgs(
            "source dialog snapshots require the route page".into(),
        ));
    }
    if !(-1..=2).contains(&source_dialog_click_tab) {
        return Err(Error::InvalidArgs(
            "unknown source dialog click target".into(),
        ));
    }
    if source_dialog_click_tab >= 0 && page_index != 0 {
        return Err(Error::InvalidArgs(
            "source dialog click simulation requires the route page".into(),
        ));
    }
    if source_menu_click && page_index != 0 {
        return Err(Error::InvalidArgs(
            "source menu click simulation requires the route page".into(),
        ));
    }
    if source_meter && page_index != 0 {
        return Err(Error::InvalidArgs(
            "source meter snapshots require the route page".into(),
        ));
    }
    if source_dialog_reopen && (page_index != 0 || source_dialog_tab < 0) {
        return Err(Error::InvalidArgs(
            "source dialog reopen snapshots require a route page and source tab".into(),
        ));
    }
    // 每次快照都创建独立窗口，避免复用主窗口导致上一次场景的 Slint 属性或模型残留。
    let ui = MainWindow::new().map_err(|e| Error::InvalidArgs(e.to_string()))?;
    // 配置只作为 fixture 的数据来源；Rc/RefCell 让后续桥接初始化共享同一份临时状态。
    let persisted_state = Rc::new(RefCell::new(config::load()));
    // 快照只构造内存 fixture，不把旧媒体时长补全写回用户配置。
    let _ = hydrate_audio_durations(&persisted_state);
    let persisted = persisted_state.borrow().clone();
    ui.set_all_devices(persisted.output.all_devices);
    ui.set_set_default_mic(persisted.output.set_default_mic);
    ui.set_set_default_render(persisted.output.set_default_render);
    ui.set_set_default_communications(persisted.output.set_default_communications);
    // 空白分类场景必须使用空模型，不能从真实库借一条数据，否则右键命中测试会失真。
    let (categories, audio_entries, audio_category) = if simulate_empty_category_right_click {
        (Vec::new(), Vec::new(), String::new())
    // 分类行场景使用固定层级数据，保证分页和右键坐标在不同用户配置下仍可复现。
    } else if simulate_category_row_right_click {
        let categories = vec![
            "分类1".into(),
            "分类2".into(),
            "分类3".into(),
            "分类2/分类1".into(),
        ];
        let category = crate::library::category_paths(&categories, &[])[0].clone();
        (categories, Vec::new(), category)
    // 普通快照沿用持久化库的副本；副本随后只被 UI 读取，不改变磁盘配置。
    } else {
        let categories = persisted.audio_categories.clone();
        let entries = persisted.audio_entries.clone();
        let category = crate::library::category_paths(&categories, &entries)
            .first()
            .cloned()
            .unwrap_or_default();
        (categories, entries, category)
    };
    // 仅把场景所需的库数据装入 UiCache，保持快照与真实运行缓存的生命周期隔离。
    let cache = Rc::new(RefCell::new(UiCache {
        audio_categories: categories,
        audio_entries,
        audio_category,
        ..UiCache::default()
    }));
    ui.set_page(page_index);
    if page_index == 2 && settings_section >= 0 {
        ui.set_settings_section_index(settings_section);
    }
    ui.set_audio_categories(ModelRc::new(VecModel::from(
        super::super::library::audio_categories(&cache),
    )));
    // 播放页需要设备下标和预览状态，先补设备模型再把持久化设备 ID 映射成当前列表下标。
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
    // 路由/设置页共享真实选择恢复逻辑，随后只在路由页构造最小源行 fixture。
    } else {
        refill(&ui, &cache, true);
        restore_saved_selection(&ui, &cache, &persisted);
        ui.set_mix_mic(persisted.route.mix_mic);
        ui.set_master_volume(persisted.mix.master * 100.0);
        ui.set_preview_enabled(persisted.preview_enabled);
        ui.set_preview_volume(persisted.preview_volume * 100.0);
        // 快照只放入第一条进程、文件和可用麦克风，足以覆盖布局而不依赖完整设备枚举。
        if page_index == 0 {
            let cache = cache.borrow();
            let mut rows = super::super::sources::SourceRows::default();
            if let Some(process) = cache.processes.first() {
                rows.push_process(super::super::sources::ProcessRowInput {
                    name: process.name.clone(),
                    pid: process.pid,
                    status: "可添加".into(),
                    icon_image: crate::platform::icon::process_icon(process.pid),
                    active: false,
                    volume: persisted.mix.process,
                    peak: 0.0,
                    output_index: 0,
                });
            }
            if let Some(entry) = cache.audio_entries.first() {
                let detail = format!(
                    "{} · {}",
                    crate::library::category_leaf(&entry.category),
                    super::super::format::format_duration_hns(entry.duration_hns)
                );
                rows.push_file(super::super::sources::FileRowInput {
                    name: entry.name.clone(),
                    path: entry.path.clone(),
                    detail,
                    status: "待播放".into(),
                    active: false,
                    paused: false,
                    volume: persisted.mix.sfx,
                    peak: 0.0,
                    output_index: 0,
                });
            }
            if let Some(mic_id) = persisted.output.mic_id.as_deref()
                && let Some(index) = cache.mic_ids.iter().position(|id| id == mic_id)
                && let Some(name) = cache.mic_names.get(index)
            {
                rows.push_microphone(super::super::sources::MicrophoneRowInput {
                    name: name.clone(),
                    status: if persisted.route.mix_mic {
                        "已启用".into()
                    } else {
                        "未启用".into()
                    },
                    active: persisted.route.mix_mic,
                    volume: persisted.mix.mic,
                    peak: 0.0,
                    output_index: 0,
                });
            }
            rows.apply(&ui);
            // 电平表模型按“每行 32 个采样点”展开；这里填零表示 fixture 没有实时音频线程。
            if source_meter {
                let source_count = ui.get_source_titles().row_count();
                let source_history = vec![0.0; source_count * 32];
                let source_decibels = vec![slint::SharedString::from("-∞ dB"); source_count];
                ui.set_source_meter_history(ModelRc::new(VecModel::from(source_history)));
                ui.set_source_meter_dbs(ModelRc::new(VecModel::from(source_decibels)));
                ui.set_master_meter_history(ModelRc::new(VecModel::from(vec![0.0; 32])));
                ui.set_master_peak_value(0.0);
                ui.set_master_peak_db("-∞ dB".into());
                ui.set_source_meter_open(true);
            }
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
    // 诊断示例与实时诊断互斥：前者验证展示状态，后者验证同步真实 Hub 快照。
    if diagnostics || diagnostics_log || diagnostics_badge {
        seed_diagnostics(&ui, &persisted, diagnostics_log, diagnostics_badge);
    }
    if diagnostics_live {
        let last = std::cell::Cell::new(usize::MAX);
        diagnostics::sync(&ui, &last);
    }
    ui.window().set_size(slint::LogicalSize::new(1280.0, 800.0));
    let source_dialog =
        // 对话框窗口单独同步生命周期和缓存，避免把子窗口的模型绑定到主窗口句柄。
        if source_dialog_tab >= 0 || source_dialog_click_tab >= 0 || source_dialog_reopen {
            super::super::library::update_audio_catalog(&ui, &cache);
            let initial_tab = if source_dialog_click_tab >= 0 {
                (source_dialog_click_tab + 1) % 3
            } else {
                source_dialog_tab
            };
            let dialog =
                AddSourceWindow::new().map_err(|error| Error::InvalidArgs(error.to_string()))?;
            super::super::window_lifecycle::sync_add_source_window(
                &ui,
                &dialog,
                &cache,
                &persisted_state,
            );
            dialog.set_selected_tab(initial_tab);
            Some(dialog)
        } else {
            None
        };
    ui.window()
        .show()
        .map_err(|error| Error::InvalidArgs(error.to_string()))?;
    if let Some(dialog) = source_dialog.as_ref() {
        dialog
            .show()
            .map_err(|error| Error::InvalidArgs(error.to_string()))?;
    }
    // 隐藏后重开并比较完整像素，验证窗口表面刷新而不是只验证可见属性。
    if source_dialog_reopen {
        let dialog = source_dialog
            .as_ref()
            .ok_or_else(|| Error::InvalidArgs("source dialog window was not created".into()))?;
        let first_frame = dialog
            .window()
            .take_snapshot()
            .map_err(|error| Error::InvalidArgs(error.to_string()))?;
        dialog
            .hide()
            .map_err(|error| Error::InvalidArgs(error.to_string()))?;
        dialog
            .show()
            .map_err(|error| Error::InvalidArgs(error.to_string()))?;
        dialog.set_refresh_surface(!dialog.get_refresh_surface());
        dialog.window().request_redraw();
        let second_frame = dialog
            .window()
            .take_snapshot()
            .map_err(|error| Error::InvalidArgs(error.to_string()))?;
        if first_frame.width() != second_frame.width()
            || first_frame.height() != second_frame.height()
            || first_frame.as_bytes() != second_frame.as_bytes()
        {
            return Err(Error::InvalidArgs(
                "source window rendering changed after hide/show".into(),
            ));
        }
    }
    // 先取一帧再派发点击，确保目标卡片已布局；选中下标是点击副作用的验收信号。
    if source_dialog_click_tab >= 0 {
        let dialog = source_dialog
            .as_ref()
            .ok_or_else(|| Error::InvalidArgs("source dialog window was not created".into()))?;
        let _ = dialog
            .window()
            .take_snapshot()
            .map_err(|error| Error::InvalidArgs(error.to_string()))?;
        let positions = [127.0, 360.0, 593.0];
        dispatch_left_click(
            dialog,
            slint::LogicalPosition::new(positions[source_dialog_click_tab as usize], 90.0),
        );
        if dialog.get_selected_tab() != source_dialog_click_tab {
            return Err(Error::InvalidArgs(
                "source dialog type card did not switch to the clicked tab".into(),
            ));
        }
    }
    // 通过真实窗口事件验证 PopupWindow 的命中区域，而不是直接设置其 open 属性。
    if source_menu_click {
        let _ = ui
            .window()
            .take_snapshot()
            .map_err(|error| Error::InvalidArgs(error.to_string()))?;
        dispatch_left_click(&ui, slint::LogicalPosition::new(1208.0, 167.0));
        let _ = ui
            .window()
            .take_snapshot()
            .map_err(|error| Error::InvalidArgs(error.to_string()))?;
    }
    // 右键模拟必须经过事件分发，才能覆盖空白区域与分类行各自的命中逻辑。
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
    // 分类行右键还校验行下标，防止菜单虽然打开却绑定到上一条分类。
    if simulate_category_row_right_click {
        let _ = ui
            .window()
            .take_snapshot()
            .map_err(|e| Error::InvalidArgs(e.to_string()))?;
        dispatch_right_click(&ui, slint::LogicalPosition::new(100.0, 202.0));
        if !ui.get_audio_category_menu_open() || ui.get_audio_category_index() != 2 {
            return Err(Error::InvalidArgs(
                "category row right-click did not open its matching menu".into(),
            ));
        }
    }
    // 软件渲染器按需渲染：先压一次事件循环，再取快照。
    let _ = slint::invoke_from_event_loop(|| {});
    // 子窗口优先取快照；没有子窗口时才保存主窗口，保证输出对应用户请求的场景。
    let snapshot = if let Some(dialog) = source_dialog.as_ref() {
        dialog.window().take_snapshot()
    } else {
        ui.window().take_snapshot()
    }
    .map_err(|error| Error::InvalidArgs(error.to_string()))?;
    write_snapshot(path, &snapshot)?;
    Ok(())
}

/// 将 Slint 快照写成宽度、高度和 RGBA 三段连续数据。
fn write_snapshot(
    path: &str,
    snapshot: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
) -> Result<(), Error> {
    // 文件格式固定为 little-endian 宽度、高度和紧随其后的 RGBA 像素，供外部脚本无需图像库读取。
    let mut bytes = Vec::with_capacity(8 + snapshot.as_bytes().len());
    bytes.extend_from_slice(&snapshot.width().to_le_bytes());
    bytes.extend_from_slice(&snapshot.height().to_le_bytes());
    bytes.extend_from_slice(snapshot.as_bytes());
    // 一次性写入完整缓冲区，避免输出半张快照让像素回归工具误判。
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
    // 示例数据先清空全局 Hub，确保快照不叠加上一次运行残留的诊断条目。
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
    // 先按正常桥接路径同步模型，再覆盖面板开关/选中项，保证徽标和列表来自同一份快照。
    diagnostics::init(ui, persisted);
    // 新 Cell 代表本次渲染的同步游标；不能复用长期 UI 状态，否则首帧可能被跳过。
    let last = std::cell::Cell::new(usize::MAX);
    diagnostics::sync(ui, &last);
    // 这些属性只控制展示场景：badge_only 保留徽标，log_tab 切换日志页，不改变 Hub 数据。
    let state = ui.global::<DiagState>();
    state.set_open(!badge_only);
    state.set_selected(2);
    state.set_tab(log_tab as i32);
}

/// 向主窗口派发完整右键序列，使 Slint 的命中测试和菜单副作用按真实交互执行。
fn dispatch_right_click(ui: &MainWindow, position: slint::LogicalPosition) {
    let window = ui.window();
    // Slint 需要移动、按下、释放完整序列才能触发右键回调；只发按下会留下悬挂指针状态。
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

/// 向任意 Slint 组件窗口派发完整左键序列，用于测试卡片点击后的状态切换。
fn dispatch_left_click<T: ComponentHandle>(ui: &T, position: slint::LogicalPosition) {
    let window = ui.window();
    // 左键同样发送完整生命周期，组件才能执行点击回调而不是仅改变 hover 状态。
    window.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    window.dispatch_event(slint::platform::WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    window.dispatch_event(slint::platform::WindowEvent::PointerReleased {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
}

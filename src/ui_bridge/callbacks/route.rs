//! route 相关 Slint 回调注册：把 UI 事件翻译为缓存、持久化配置和音频引擎操作。
/// 路由音频卡片的分类队列、前后切歌和播放模式。
pub(super) mod audio_queue;
/// 后台刷新进程/设备列表，避免路由注册逻辑直接阻塞 UI。
mod refresh;
/// 源卡片的类型映射、槽位状态与运行中热切换。
pub(super) mod sources;

use super::super::*;
use slint::{ComponentHandle, Model};
/// 绑定路由页的设备、源列表和混音相关回调；每个回调只更新对应的缓存、配置或引擎槽位。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    refresh::register(ui, state);
    register_1(ui, state);
    register_2(ui, state);
    sources::register(ui, state);
    audio_queue::register(ui, state);
}

/// 注册路由页第一组回调：总音量、多设备开关、设备勾选和默认麦克风选项。
fn register_1(ui: &MainWindow, state: &AppState) {
    let engine = state.engine.clone();
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    {
        let save_queue = state.save_queue.clone();
        let engine = engine.clone();
        let app_config = app_config.clone();
        ui.on_master_volume_edited(move |v| {
            // 总音量回调同时更新引擎即时状态和 AppConfig 持久化值，两者分别服务运行时与下次启动。
            let value = (v / 100.0).clamp(0.0, 1.0);
            // Slint 使用百分比，AudioEngine 与配置统一保存为 0.0..=1.0 的线性增益。
            engine.set_master_volume(value);
            // 先调引擎让当前播放立即生效，再写配置，避免仅保存设置却不改变现有会话。
            let mut config = app_config.borrow_mut();
            config.mix.master = value;
            save_queue.schedule();
            // 滑块事件只排入去抖保存队列，实际文件写入在队列线程中合并完成。
        });
    }
    {
        let ui_weak = ui.as_weak();
        let app_config = app_config.clone();
        let save_queue = state.save_queue.clone();
        ui.on_all_devices_toggled(move |on| {
            // 全设备开关是输出设备列表的筛选条件；保存后主动刷新列表以重建 UI 下标。
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_all_devices(on);
            // UI 属性是本次事件的即时反馈，配置则是下一次启动恢复的权威持久化来源。
            app_config.borrow_mut().output.all_devices = on;
            save_queue.schedule();
            ui.invoke_refresh();
            // 刷新必须在写入开关后发起，否则后台枚举会使用旧筛选条件。
        });
    }
    {
        let cache = cache.clone();
        let app_config = app_config.clone();
        let save_queue = state.save_queue.clone();
        let ui_weak = ui.as_weak();
        ui.on_device_toggled(move |index, checked| {
            // 勾选按可见端点身份保存；虚拟麦与配对扬声器即使指向同一 render 也互不牵连。
            let mut cache = cache.borrow_mut();
            // 下标可能因刷新而过期；无对应缓存项时忽略事件，避免破坏已选设备集合。
            let Some(slot) = cache.device_selected.get_mut(index as usize) else {
                return;
            };
            *slot = checked;
            cache.selected_device_ids = cache
                .dests
                .iter()
                .zip(&cache.device_selected)
                .filter(|(_, selected)| **selected)
                .map(|(device, _)| device.selection_id().to_owned())
                .collect();
            app_config.borrow_mut().output.device_ids = cache.selected_device_ids.clone();
            let selected = cache.device_selected.clone();
            drop(cache);
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_device_selected(slint::ModelRc::new(slint::VecModel::from(selected)));
            }
            // 只保存列表项稳定 ID；保存队列负责合并快速连续的勾选事件。
            save_queue.schedule();
        });
    }
    {
        let app_config = app_config.clone();
        let save_queue = state.save_queue.clone();
        ui.on_set_default_mic_toggled(move |on| {
            // 默认麦克风开关只改变路由策略配置，不在此回调中直接操作设备枚举或引擎线程。
            app_config.borrow_mut().output.set_default_mic = on;
            save_queue.schedule();
            // 采用去抖写盘，保证切换设置时 UI 线程只承担内存状态更新。
        });
    }
}

/// 注册第二组路由回调：默认输出策略、添加进程以及麦克风混音热切换。
fn register_2(ui: &MainWindow, state: &AppState) {
    let cache = state.cache.clone();
    let app_config = state.app_config.clone();
    let save_queue = state.save_queue.clone();
    {
        let app_config = app_config.clone();
        let save_queue = save_queue.clone();
        ui.on_set_default_render_toggled(move |on| {
            // 默认输出策略属于持久化路由选项；回调不改动当前设备快照，避免触发无关刷新。
            app_config.borrow_mut().output.set_default_render = on;
            save_queue.schedule();
            // 仅安排延迟写盘，退出时由统一保存流程补齐最后一次配置变更。
        });
    }
    {
        let app_config = app_config.clone();
        let save_queue = save_queue.clone();
        ui.on_set_default_communications_toggled(move |on| {
            // 默认通信设备策略与默认渲染策略独立保存，避免切换一个选项覆盖另一个选项。
            app_config.borrow_mut().output.set_default_communications = on;
            save_queue.schedule();
            // 配置对象集中持有跨会话状态，保存队列负责异步、去抖地落盘。
        });
    }
    {
        let ui_weak = ui.as_weak();
        let state = state.clone();
        // -1 表示新增源；非负值表示替换已有行，供后续按源类型计算槽位。
        ui.on_source_add_requested(move || {
            // 添加源窗口复用同一回调：替换索引为 -1 表示新增，否则表示编辑已有卡片。
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_source_replace_index(-1);
                // 先重置替换上下文，再打开窗口，防止上一次编辑的槽位泄漏到本次新增。
                ui.set_source_add_tab(0);
                super::super::window_lifecycle::open_add_source_window(&ui, &state);
            }
        });
    }
    {
        let engine = state.engine.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let app_config = app_config.clone();
        let save_queue = save_queue.clone();
        let state_for_update = state.clone();
        // 进程槽位按 UI 行中的类型 0 计数，不能直接使用混合源列表的行号。
        ui.on_process_add(move || {
            // 该回调把 UI 选择转换为进程槽位变更，并在运行中只热替换进程采集源。
            let Some(ui) = ui_weak.upgrade() else { return };
            let index = ui.get_process_index().max(0) as usize;
            // process_index 只索引 pids 列表；混合源列表中的行号不能直接当作进程槽位。
            let Some(pid) = cache
                .borrow()
                .pids
                .get(index)
                .copied()
                .filter(|pid| *pid != 0)
            else {
                return;
            };
            let replace_index = ui.get_source_replace_index();
            // 替换路径先把混合源行映射到 process 槽位，再校验类型与缓存长度以拒绝陈旧事件。
            let mut cache_state = cache.borrow_mut();
            if replace_index >= 0 {
                let source_kinds = ui.get_source_kinds();
                let source_index = replace_index as usize;
                let slot = (0..source_index)
                    // 统计目标行之前的进程行数，得到 selected_pids/process_volumes 使用的紧凑下标。
                    .filter(|row| source_kinds.row_data(*row) == Some(0))
                    .count();
                if source_kinds.row_data(source_index) != Some(0)
                    || slot >= cache_state.selected_pids.len()
                {
                    return;
                }
                // 替换时排除被替换槽位，避免同一进程同时出现在两个卡片中。
                if sources::duplicate_process(&cache_state.selected_pids, pid, Some(slot)) {
                    ui.set_error_text("该进程已经添加，不能创建重复音频源".into());
                    return;
                }
                cache_state.selected_pids[slot] = pid;
                // 只替换 PID，不重建整个集合，从而保留该卡片已有的音量、静音和输出设备状态。
                cache_state
                    .disabled_pids
                    .retain(|disabled| *disabled != pid);
            } else {
                // 新增进程同样拒绝重复 PID，保证一个进程只占一个独立采集槽。
                // 新增槽位同时初始化其独立运行时状态，避免新卡片复用其他进程的配置。
                if sources::duplicate_process(&cache_state.selected_pids, pid, None) {
                    ui.set_error_text("该进程已经添加，不能创建重复音频源".into());
                    return;
                }
                cache_state.selected_pids.push(pid);
                cache_state
                    .process_volumes
                    .push(app_config.borrow().mix.process);
                cache_state.process_muted_volumes.push(None);
            }
            drop(cache_state);
            // 释放缓存借用后再调用引擎和 UI 更新，避免回调内出现 RefCell 重入借用。
            ui.set_source_replace_index(-1);
            // 运行中只替换进程源集合，保留已打开的输出设备与其他源。
            // 运行中的引擎只接收有效进程集合；禁用 PID 保留在缓存中供 UI 恢复，不参与采集。
            if ui.get_running() {
                let wanted = {
                    let cache_state = cache.borrow();
                    cache_state
                        .selected_pids
                        .iter()
                        .copied()
                        .filter(|pid| !cache_state.disabled_pids.contains(pid))
                        .collect::<Vec<_>>()
                };
                let _ = engine.replace_process_sources(&wanted);
            }
            super::super::persist::save_process_selection(&cache, &app_config, &save_queue);
            // 选择集合变更后先排入配置写盘，再刷新源卡片，使持久化和展示使用同一份缓存快照。
            super::super::sources::update(&ui, &state_for_update);
        });
    }
    {
        let engine = state.engine.clone();
        let ui_weak = ui.as_weak();
        let cache = cache.clone();
        let save_queue = save_queue.clone();
        let app_config = app_config.clone();
        let state_for_update = state.clone();
        // 类型 2 是麦克风源；开关变化会触发配置保存，并可在运行中热切换。
        ui.on_mic_mix_changed(move |on| {
            // 麦克风开关同时影响配置、源卡片可见性和运行中的混音链，三者必须按同一有效 ID 决定。
            let Some(ui) = ui_weak.upgrade() else { return };
            let mic_id = cache
                // mic_index 只在当前缓存快照内有效；空字符串或越界都视为没有可用麦克风。
                .borrow()
                .mic_ids
                .get(ui.get_mic_index().max(0) as usize)
                .filter(|id| !id.is_empty())
                .cloned();
            let mix_mic = on && mic_id.is_some();
            // 没有有效设备 ID 时强制关闭混音，避免 UI 勾选状态与实际引擎输入不一致。
            if mix_mic {
                cache.borrow_mut().mic_source_hidden = false;
            }
            ui.set_mix_mic(mix_mic);
            // 先回写规范化后的开关值，再保存配置，保证 UI 不显示无法实际生效的“开启”。
            let mut config = app_config.borrow_mut();
            config.route.mix_mic = mix_mic;
            config.route.mic_id = mic_id.clone();
            save_queue.schedule();
            // 配置变更进入去抖队列；不要在 UI 回调中直接写文件或等待后台 IO。
            drop(config);
            // 运行中热切换麦克风：停旧开新，复用已建立的输出。
            // 运行中只替换麦克风输入，复用已有输出链；失败时保留错误诊断而不破坏其他源。
            if ui.get_running() {
                let source = if mix_mic { mic_id.as_deref() } else { None };
                if let Err(error) = engine.replace_mic_source(source) {
                    tracing::warn!("热切换麦克风失败：{error}");
                    super::super::diagnostics::report_error(&ui, &error);
                }
            }
            super::super::sources::update(&ui, &state_for_update);
            // 最后刷新源模型，让隐藏状态和麦克风标签与刚写入的缓存保持一致。
        });
    }
}

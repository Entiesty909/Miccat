//! 恢复与保存 Slint 桥接层持久化状态。

use super::*;
use crate::audio::policy;

use slint::{Timer, TimerMode};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// 配置保存线程接收的命令；快照在 UI 线程生成，写线程只负责落盘。
enum PersistCommand {
    /// 防抖触发的一般保存，不需要等待完成。
    Save(config::AppConfig),
    /// 退出前保存并通过通道通知调用方已经写完。
    Flush(config::AppConfig, SyncSender<()>),
    /// 消费完队列中的命令后结束写线程。
    Stop,
}

/// 通过单写线程保存配置，并把短时间内的 UI 修改合并为一次写盘。
pub(super) struct SaveQueue {
    /// AppConfig 的 UI 线程权威，用于生成写盘快照。
    app_config: Rc<RefCell<config::AppConfig>>,
    /// 只存在于 UI 线程的防抖计时器。
    timer: RefCell<Timer>,
    /// 向单写线程发送不可变配置快照的通道。
    sender: Sender<PersistCommand>,
    /// 保存线程句柄；退出时取出并 join，确保资源回收。
    worker: RefCell<Option<JoinHandle<()>>>,
}

/// 保存队列的 UI 线程端句柄；计时器只产生不可变快照，避免写线程借用 UI 状态。
impl SaveQueue {
    /// 启动配置写线程和 300 ms 的末次修改防抖计时器。
    pub(super) fn new(app_config: Rc<RefCell<config::AppConfig>>) -> Result<Rc<Self>, Error> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("miccat-config-save".into())
            .spawn(move || save_worker(receiver))?;
        Ok(Rc::new(Self {
            app_config,
            timer: RefCell::new(Timer::default()),
            sender,
            worker: RefCell::new(Some(worker)),
        }))
    }

    /// 将计时器推迟到最后一次修改之后 300 ms。
    pub(super) fn schedule(&self) {
        // 每次重新安排都取消旧计时器，集中保存短时间连续发生的 UI 修改。
        let sender = self.sender.clone();
        let app_config = self.app_config.clone();
        let timer = self.timer.borrow_mut();
        timer.stop();
        timer.start(
            TimerMode::SingleShot,
            Duration::from_millis(300),
            move || {
                // 计时器回调只复制配置；实际文件 IO 固定在后台线程，避免阻塞 UI 事件循环。
                let snapshot = app_config.borrow().clone();
                if sender.send(PersistCommand::Save(snapshot)).is_err() {
                    tracing::warn!("config save worker stopped before debounce fired");
                }
            },
        );
    }

    /// 写入退出时的最终配置快照，并等待此前排队的写入完成。
    pub(super) fn flush(&self) {
        // 先停掉防抖回调，再入队最终快照；通道 FIFO 保证此确认发生在先前保存之后。
        self.timer.borrow_mut().stop();
        let snapshot = self.app_config.borrow().clone();
        let (complete, finished) = mpsc::sync_channel(0);
        if self
            .sender
            .send(PersistCommand::Flush(snapshot, complete))
            .is_err()
        {
            tracing::warn!("config save worker stopped before flush");
            return;
        }
        // 零容量应答通道只在工作线程完成当前配置写盘后交付，避免退出时丢失尾部修改。
        if finished.recv().is_err() {
            tracing::warn!("config save worker stopped during flush");
        }
    }

    /// 关闭配置写线程并等待退出。
    pub(super) fn shutdown(&self) {
        self.timer.borrow_mut().stop();
        let _ = self.sender.send(PersistCommand::Stop);
        // Stop 排在已发送的保存命令之后；join 等待工作线程消费队列并退出。
        if let Some(worker) = self.worker.borrow_mut().take()
            && worker.join().is_err()
        {
            tracing::warn!("config save worker panicked");
        }
    }
}

/// 在单独线程按接收顺序执行保存，`Flush` 的应答保证退出时写入已完成。
fn save_worker(receiver: Receiver<PersistCommand>) {
    while let Ok(command) = receiver.recv() {
        match command {
            // Save 无需同步应答；Flush 则在写盘后确认，供退出路径等待。
            PersistCommand::Save(config) => config::save(&config),
            PersistCommand::Flush(config, complete) => {
                config::save(&config);
                let _ = complete.send(());
            }
            PersistCommand::Stop => break,
        }
    }
}

/// 启动时为旧配置补齐缺失的媒体时长；失效文件保留 0 并显示占位符。
pub(super) fn hydrate_audio_durations(app_config: &Rc<RefCell<config::AppConfig>>) -> bool {
    let mut changed = false;
    // 调用方据此决定是否需要把补齐后的旧配置重新写回磁盘。
    let mut config = app_config.borrow_mut();
    // 只修补缺失元数据，保留用户已有值；验证失败时不制造虚假时长。
    for entry in &mut config.audio_entries {
        if entry.duration_hns != 0
            && entry.file_size_bytes != 0
            && entry.sample_rate != 0
            && entry.channels != 0
        {
            continue;
        }
        // 只接受可重新验证的文件元数据；路径失效时维持原值，避免写入猜测数据。
        if let Ok(info) = crate::library::validate_file(std::path::Path::new(&entry.path)) {
            if entry.duration_hns == 0 {
                entry.duration_hns = info.duration_hns;
            }
            if entry.file_size_bytes == 0 {
                entry.file_size_bytes = std::fs::metadata(&entry.path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
            }
            if entry.sample_rate == 0 {
                entry.sample_rate = info.sample_rate;
            }
            if entry.channels == 0 {
                entry.channels = info.channels;
            }
            changed = true;
        }
    }
    changed
}

/// 按持久化的 ID / 名称恢复进程、输出设备与物理麦选择。
pub(super) fn restore_saved_selection(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    saved: &config::AppConfig,
) {
    let mut selected_pids = Vec::new();
    {
        let cache = cache.borrow();
        // 先用稳定标识恢复索引，避免刷新后的列表顺序变化误选其他进程。
        // 新配置按名称列表恢复多个卡片；兼容旧版仅保存单个 process_name 的配置。
        let names = if saved.route.process_names.is_empty() {
            saved.route.process_name.iter().cloned().collect::<Vec<_>>()
        } else {
            saved.route.process_names.clone()
        };
        for name in names {
            if let Some(process) = cache
                .processes
                .iter()
                .find(|process| process.name.eq_ignore_ascii_case(&name))
            {
                selected_pids.push(process.pid);
            }
        }
        if let Some(pid) = selected_pids.first()
            && let Some(index) = cache.pids.iter().position(|candidate| candidate == pid)
        {
            ui.set_process_index(index as i32);
        }
        // 优先使用稳定设备 ID，旧配置或设备更换时再用名称尽力匹配。
        let device_index = saved
            .output
            .device_id
            .as_deref()
            .and_then(|id| cache.dests.iter().position(|device| device.render_id == id))
            .or_else(|| {
                saved
                    .output
                    .device_name
                    .as_deref()
                    .and_then(|name| cache.dests.iter().position(|device| device.label == name))
            });
        if let Some(index) = device_index {
            ui.set_device_index(index as i32);
        }
        if let Some(id) = saved.route.mic_id.as_deref()
            && let Some(index) = cache.mic_ids.iter().position(|candidate| candidate == id)
        {
            ui.set_mic_index(index as i32);
        }
    }
    let mut cache_state = cache.borrow_mut();
    // 将已解析的选择集中更新到缓存，之后所有卡片状态都以这些槽位为准。
    cache_state.selected_pids = selected_pids;
    cache_state.disabled_pids = saved
        .route
        .disabled_process_slots
        .iter()
        .filter_map(|slot| cache_state.selected_pids.get(*slot).copied())
        .collect();
    // 配置保存的是路径，缓存保存的是库内下标；先做路径匹配再重建平行槽位数组。
    let audio_sources = saved
        .route
        .audio_source_paths
        .iter()
        .enumerate()
        .filter_map(|(saved_slot, path)| {
            cache_state
                .audio_entries
                .iter()
                .position(|entry| entry.path == *path)
                .map(|index| (saved_slot, index))
        })
        .collect::<Vec<_>>();
    cache_state.route_audio_entries = audio_sources.iter().map(|(_, index)| *index).collect();
    // 丢失的音频路径会被跳过；启用标记按原保存槽读取，避免与压缩后的索引错位。
    cache_state.route_audio_enabled = audio_sources
        .iter()
        .map(|(saved_slot, _)| {
            saved
                .route
                .audio_source_enabled
                .get(*saved_slot)
                .copied()
                .unwrap_or(false)
        })
        .collect();
    cache_state.audio_play_modes = audio_sources
        .iter()
        .map(|(saved_slot, _)| {
            saved
                .route
                .audio_source_play_modes
                .get(*saved_slot)
                .copied()
                .filter(|mode| *mode < 4)
                .unwrap_or(0)
        })
        .collect();
    cache_state.audio_finished_handled = vec![false; audio_sources.len()];
    cache_state.route_audio_entry = cache_state.route_audio_entries.first().copied();
    // 单卡片字段仅作兼容/界面默认值，多卡片列表才是实际路由状态。
    cache_state.route_file_enabled = cache_state
        .route_audio_enabled
        .iter()
        .any(|enabled| *enabled);
    cache_state.file_source_hidden = cache_state.route_audio_entries.is_empty();
    let process_count = cache_state.selected_pids.len();
    // 按当前已解析卡片数修齐平行数组，保证槽位对齐；旧配置缺项时使用全局默认值。
    cache_state.process_volumes.truncate(process_count);
    cache_state
        .process_volumes
        .resize(process_count, saved.mix.process);
    cache_state.process_muted_volumes.truncate(process_count);
    cache_state
        .process_muted_volumes
        .resize(process_count, None);
    let audio_count = cache_state.route_audio_entries.len();
    cache_state.audio_volumes.truncate(audio_count);
    cache_state.audio_volumes.resize(audio_count, saved.mix.sfx);
    cache_state.audio_muted_volumes.truncate(audio_count);
    cache_state.audio_muted_volumes.resize(audio_count, None);
    // 释放缓存借用后再刷新界面文本，避免更新 UI 时重入缓存访问。
    drop(cache_state);
    update_output_text(ui, cache);
}
/// 把当前选择与混音值合并到持久化配置。
pub(super) fn persist_config(
    ui: &MainWindow,
    cache: &Rc<RefCell<UiCache>>,
    app_config: &Rc<RefCell<config::AppConfig>>,
) {
    let c = cache.borrow();
    // UI 下标只用于查缓存，落盘时改存设备 ID、路径和稳定名称，避免列表重排破坏配置。
    let mut saved = app_config.borrow_mut();
    // 先覆盖配置中的库数据，再把界面下标转换为稳定标识保存。
    saved.audio_entries = c.audio_entries.clone();
    saved.audio_categories = c.audio_categories.clone();
    saved.output.all_devices = ui.get_all_devices();
    saved.output.device_ids = if c.selected_device_ids.is_empty() {
        c.dests
            .get(ui.get_device_index() as usize)
            .map(|device| vec![device.selection_id().to_owned()])
            .unwrap_or_default()
    } else {
        c.selected_device_ids.clone()
    };
    saved.output.set_default_communications = ui.get_set_default_communications();
    saved.route.mix_mic = ui.get_mix_mic();
    saved.route.mic_id = c
        .mic_ids
        .get(ui.get_mic_index() as usize)
        .filter(|id| !id.is_empty())
        .cloned();
    saved.output.device_name = c
        .dests
        .get(ui.get_device_index() as usize)
        .map(|d| d.label.clone());
    let mic_index = ui.get_mic_index() as usize;
    saved.output.mic_id = c.mic_ids.get(mic_index).filter(|v| !v.is_empty()).cloned();
    saved.output.mic_name = c
        .mic_names
        .get(mic_index)
        .filter(|v| !v.is_empty())
        .cloned();
    let pid = c.pids.get(ui.get_process_index() as usize).copied();
    // 保留单进程旧字段，同时由槽位列表记录全部卡片的稳定名称。
    saved.route.process_name = pid.and_then(|pid| {
        c.processes
            .iter()
            .find(|p| p.pid == pid)
            .map(|p| p.name.clone())
    });
    // disabled_process_slots 与进程卡片槽位同序，不能直接把 PID 当作持久化下标。
    saved.route.disabled_process_slots = c
        .selected_pids
        .iter()
        .enumerate()
        .filter_map(|(slot, pid)| c.disabled_pids.contains(pid).then_some(slot))
        .collect();
    saved.route.audio_source_paths = c
        .route_audio_entries
        .iter()
        .filter_map(|index| c.audio_entries.get(*index).map(|entry| entry.path.clone()))
        .collect();
    saved.route.audio_source_enabled = c.route_audio_enabled.clone();
    saved.route.audio_source_play_modes = c.audio_play_modes.clone();
    saved.mix.master = (ui.get_master_volume() / 100.0).clamp(0.0, 1.0);
    // 主音量 UI 使用百分比，配置统一保存为限制在 [0, 1] 的线性值。
    saved.mix.process_volumes = c.process_volumes.clone();
    saved.mix.sfx_volumes = c.audio_volumes.clone();
}

/// 将进程、音频卡片及其启用状态防抖写回配置。
pub(super) fn save_process_selection(
    cache: &Rc<RefCell<UiCache>>,
    app_config: &Rc<RefCell<config::AppConfig>>,
    save_queue: &SaveQueue,
) {
    let (names, disabled_slots, audio_paths, audio_enabled, audio_play_modes) = {
        // 配置使用稳定名称和路径，不依赖进程/音频库刷新后的瞬时列表下标。
        // 在释放缓存借用后再修改 AppConfig，避免 RefCell 重入；保存队列接收独立快照。
        let cache = cache.borrow();
        let names = cache
            .selected_pids
            .iter()
            .filter_map(|pid| {
                cache
                    .processes
                    .iter()
                    .find(|process| process.pid == *pid)
                    .map(|process| process.name.clone())
            })
            .collect::<Vec<_>>();
        let disabled_slots = cache
            .selected_pids
            .iter()
            .enumerate()
            .filter_map(|(slot, pid)| cache.disabled_pids.contains(pid).then_some(slot))
            .collect::<Vec<_>>();
        let audio_paths = cache
            .route_audio_entries
            .iter()
            .filter_map(|index| {
                cache
                    .audio_entries
                    .get(*index)
                    .map(|entry| entry.path.clone())
            })
            .collect::<Vec<_>>();
        (
            names,
            disabled_slots,
            audio_paths,
            cache.route_audio_enabled.clone(),
            cache.audio_play_modes.clone(),
        )
    };
    let mut config = app_config.borrow_mut();
    // 修改内存配置后安排防抖保存；队列稍后复制完整快照交由后台线程写盘。
    config.route.process_name = names.first().cloned();
    config.route.process_names = names;
    config.route.disabled_process_slots = disabled_slots;
    config.route.audio_source_paths = audio_paths;
    config.route.audio_source_enabled = audio_enabled;
    config.route.audio_source_play_modes = audio_play_modes;
    save_queue.schedule();
}

/// 恢复启动前被临时替换的默认麦克风，并消费一次性保存列表。
pub(super) fn restore_default_mics(saved: &Rc<RefCell<Vec<(u32, String)>>>) {
    let prev = saved.borrow_mut().split_off(0);
    // 先取走待恢复列表，使该恢复操作至多执行一次，即使系统调用失败也不重复覆盖。
    if prev.is_empty() {
        return;
    }
    if let Err(e) = policy::restore_defaults(&prev) {
        tracing::warn!("restore default mic: {e}");
    }
}

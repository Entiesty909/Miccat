//! 恢复与保存 Slint 桥接层持久化状态。

use super::*;
use crate::audio::policy;

use slint::{Timer, TimerMode};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

enum PersistCommand {
    Save(config::AppConfig),
    Flush(config::AppConfig, SyncSender<()>),
    Stop,
}

/// 通过单写线程保存配置，并把短时间内的 UI 修改合并为一次写盘。
pub(super) struct SaveQueue {
    app_config: Rc<RefCell<config::AppConfig>>,
    timer: RefCell<Timer>,
    sender: Sender<PersistCommand>,
    worker: RefCell<Option<JoinHandle<()>>>,
}

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
        let sender = self.sender.clone();
        let app_config = self.app_config.clone();
        let timer = self.timer.borrow_mut();
        timer.stop();
        timer.start(
            TimerMode::SingleShot,
            Duration::from_millis(300),
            move || {
                let snapshot = app_config.borrow().clone();
                if sender.send(PersistCommand::Save(snapshot)).is_err() {
                    tracing::warn!("config save worker stopped before debounce fired");
                }
            },
        );
    }

    /// 写入退出时的最终配置快照，并等待此前排队的写入完成。
    pub(super) fn flush(&self) {
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
        if finished.recv().is_err() {
            tracing::warn!("config save worker stopped during flush");
        }
    }

    /// 关闭配置写线程并等待退出。
    pub(super) fn shutdown(&self) {
        self.timer.borrow_mut().stop();
        let _ = self.sender.send(PersistCommand::Stop);
        if let Some(worker) = self.worker.borrow_mut().take()
            && worker.join().is_err()
        {
            tracing::warn!("config save worker panicked");
        }
    }
}

fn save_worker(receiver: Receiver<PersistCommand>) {
    while let Ok(command) = receiver.recv() {
        match command {
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
    let mut config = app_config.borrow_mut();
    for entry in &mut config.audio_entries {
        if entry.duration_hns != 0
            && entry.file_size_bytes != 0
            && entry.sample_rate != 0
            && entry.channels != 0
        {
            continue;
        }
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
    cache_state.selected_pids = selected_pids;
    let process_count = cache_state.selected_pids.len();
    cache_state
        .process_volumes
        .resize(process_count, saved.mix.process);
    cache_state
        .process_muted_volumes
        .resize(process_count, None);
    cache_state
        .process_output_ids
        .resize(process_count, saved.output.process_output_id.clone());
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
    let mut saved = app_config.borrow_mut();
    saved.audio_entries = c.audio_entries.clone();
    saved.audio_categories = c.audio_categories.clone();
    saved.output.all_devices = c.all_devices;
    saved.output.device_ids = if c.selected_device_ids.is_empty() {
        c.dests
            .get(ui.get_device_index() as usize)
            .map(|device| vec![device.render_id.clone()])
            .unwrap_or_default()
    } else {
        c.selected_device_ids.clone()
    };
    saved.output.set_default_communications = c.set_default_communications;
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
    saved.route.process_name = pid.and_then(|pid| {
        c.processes
            .iter()
            .find(|p| p.pid == pid)
            .map(|p| p.name.clone())
    });
    saved.mix.master = (ui.get_master_volume() / 100.0).clamp(0.0, 1.0);
    saved.mix.process_volumes = c.process_volumes.clone();
    saved.mix.sfx_volumes = c.audio_volumes.clone();
    saved.output.process_output_ids = c
        .process_output_ids
        .iter()
        .filter_map(|id| id.clone())
        .collect();
    saved.output.file_output_ids = c
        .audio_output_ids
        .iter()
        .filter_map(|id| id.clone())
        .collect();
}
/// 将当前已选进程名称防抖写回配置，恢复时再解析为新 PID。
pub(super) fn save_process_selection(
    cache: &Rc<RefCell<UiCache>>,
    app_config: &Rc<RefCell<config::AppConfig>>,
    save_queue: &SaveQueue,
) {
    let names = {
        let cache = cache.borrow();
        cache
            .selected_pids
            .iter()
            .filter_map(|pid| {
                cache
                    .processes
                    .iter()
                    .find(|process| process.pid == *pid)
                    .map(|process| process.name.clone())
            })
            .collect::<Vec<_>>()
    };
    let mut config = app_config.borrow_mut();
    config.route.process_name = names.first().cloned();
    config.route.process_names = names;
    save_queue.schedule();
}

pub(super) fn restore_default_mics(saved: &Rc<RefCell<Vec<(u32, String)>>>) {
    let prev = saved.borrow_mut().split_off(0);
    if prev.is_empty() {
        return;
    }
    if let Err(e) = policy::restore_defaults(&prev) {
        tracing::warn!("restore default mic: {e}");
    }
}

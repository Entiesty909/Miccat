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
            .name("processmic-config-save".into())
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
        if let Some(worker) = self.worker.borrow_mut().take() {
            if worker.join().is_err() {
                tracing::warn!("config save worker panicked");
            }
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
        if entry.duration_hns != 0 {
            continue;
        }
        if let Ok(info) = crate::audio::decode::validate_file(std::path::Path::new(&entry.path)) {
            entry.duration_hns = info.duration_hns;
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
    {
        let c = cache.borrow();
        if let Some(name) = saved.route.process_name.as_deref()
            && let Some(pid) = c
                .processes
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .map(|p| p.pid)
            && let Some(index) = c.pids.iter().position(|v| *v == pid)
        {
            ui.set_process_index(index as i32);
        }
        let device_index = saved
            .output
            .device_id
            .as_deref()
            .and_then(|id| c.dests.iter().position(|d| d.render_id == id))
            .or_else(|| {
                saved
                    .output
                    .device_name
                    .as_deref()
                    .and_then(|name| c.dests.iter().position(|d| d.label == name))
            });
        if let Some(index) = device_index {
            ui.set_device_index(index as i32);
        }
        if let Some(id) = saved.output.mic_id.as_deref()
            && let Some(index) = c.mic_ids.iter().position(|v| v == id)
        {
            ui.set_mic_index(index as i32);
        }
    }
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
    saved.output.set_default_render = c.set_default_render;
    saved.output.set_default_communications = c.set_default_communications;
    saved.route.mix_mic = ui.get_mix_mic();
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
    saved.mix.process = (ui.get_volume() / 100.0).clamp(0.0, 1.0);
    saved.mix.mic = (ui.get_mic_volume() / 100.0).clamp(0.0, 1.0);
    saved.mix.master = (ui.get_master_volume() / 100.0).clamp(0.0, 1.0);
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

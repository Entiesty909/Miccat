//! 后台枚举进程、物理麦克风和输出端点，并把结果交给 UI 线程。

use slint::ComponentHandle;
use std::sync::atomic::Ordering;
use std::thread;

use super::super::super::state::EnumeratedLists;
use super::super::{AppState, MainWindow};

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    let ui_weak = ui.as_weak();
    let list_refresh = state.list_refresh.clone();
    let cache = state.cache.clone();
    ui.on_refresh(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        if list_refresh
            .pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let include_all = cache.borrow().all_devices;
        let worker_state = list_refresh.clone();
        if let Err(error) = thread::Builder::new()
            .name("audio-lists-refresh".into())
            .spawn(move || {
                let result = refresh_lists(include_all);
                if let Ok(mut slot) = worker_state.result.lock() {
                    *slot = Some(result);
                }
            })
        {
            list_refresh.pending.store(false, Ordering::Release);
            tracing::warn!("spawn audio lists refresh thread: {error}");
            super::super::super::diagnostics::report_warning(&ui, "无法刷新音频设备和进程列表");
        }
    });
}

fn refresh_lists(include_all: bool) -> Result<EnumeratedLists, String> {
    let processes = crate::process::list_processes().map_err(|error| {
        tracing::warn!("refresh processes: {error}");
        error.user_message().to_string()
    })?;
    let mics = crate::audio::device::list_physical_mics().map_err(|error| {
        tracing::warn!("refresh microphones: {error}");
        error.user_message().to_string()
    })?;
    let destinations = crate::audio::device::list_destinations(include_all).map_err(|error| {
        tracing::warn!("refresh output devices: {error}");
        error.user_message().to_string()
    })?;
    Ok(EnumeratedLists {
        processes,
        mics,
        destinations,
    })
}

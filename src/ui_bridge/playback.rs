//! 管理音频库单条流的取消、解码与试听输出。

use super::cache::PlayingEntry;
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

/// 起一条播放流：从 `start_hns` 解码，同时喂给 SFX 混音与本机试听，并重置进度锚点。
///
/// 双击播放和拖动进度条共用它：先取消上一条解码，再按新位置重开，
/// 所以两处对“换源”的处理完全一致。
pub(super) fn start_playback(
    ui: &MainWindow,
    engine: &Rc<AudioEngine>,
    cache: &Rc<RefCell<UiCache>>,
    preview: &Arc<Mutex<Option<crate::audio::preview::PreviewEngine>>>,
    preview_volume: f32,
    entry: &config::AudioEntry,
    start_hns: u64,
) {
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(old) = cache.borrow_mut().audio_cancel.replace(cancel.clone()) {
        old.store(true, Ordering::Release);
    }
    {
        let mut c = cache.borrow_mut();
        c.audio_playing = Some(PlayingEntry {
            path: entry.path.clone(),
            name: entry.name.clone(),
            looped: entry.loop_playback,
            duration_hns: entry.duration_hns,
        });
        c.audio_anchor = Some((Instant::now(), start_hns));
    }

    let sender = engine.sfx_sender().ok();
    let preview = preview.clone();
    let ui_weak = ui.as_weak();
    let name = entry.name.clone();
    let looped = entry.loop_playback;
    let path = PathBuf::from(&entry.path);
    thread::spawn(move || {
        // 转发没开时 SFX 那边是关闭的，只用试听这条，省一次解码。
        let sfx_stream = sender.as_ref().and_then(|_| {
            crate::audio::decode::stream_to_channel(path.clone(), start_hns, looped, cancel.clone())
                .ok()
        });
        match crate::audio::decode::stream_to_channel(path, start_hns, looped, cancel) {
            Ok((info, preview_rx)) => {
                if let (Some(sender), Some((sfx_info, rx))) = (sender, sfx_stream) {
                    let _ = sender.send(crate::audio::sfx::SfxCommand::PlayStream {
                        sample_rate: sfx_info.sample_rate,
                        channels: sfx_info.channels,
                        mode: if looped {
                            crate::audio::sfx::SfxMode::Loop
                        } else {
                            crate::audio::sfx::SfxMode::Once
                        },
                        volume: 1.0,
                        rx,
                    });
                }
                if let Ok(mut guard) = preview.lock() {
                    if guard.is_none() {
                        *guard = crate::audio::preview::PreviewEngine::open_default().ok();
                        if let Some(p) = guard.as_ref() {
                            p.set_volume(preview_volume);
                        }
                    }
                    if let Some(p) = guard.as_ref() {
                        p.play_stream(info, preview_rx, looped);
                    }
                }
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_audio_current_name(name.into());
                        ui.set_audio_duration_seconds(info.duration_hns as f32 / 10_000_000.0);
                        ui.set_player_playing(true);
                        ui.set_error_text("".into());
                    }
                });
            }
            Err(error) => {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_audio_progress(0.0);
                        ui.set_audio_duration_seconds(0.0);
                        ui.set_audio_current_position("".into());
                        ui.set_player_playing(false);
                        ui.set_error_text(error.user_message().into());
                    }
                });
            }
        }
    });
}

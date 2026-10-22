//! Slint 回调按业务域注册。

mod library;
mod library_entries;
mod library_view;
mod player;
mod route;
mod route_start;
mod settings;

use super::{AppState, MainWindow};
pub(super) use route::audio_queue::poll_finished as poll_finished_route_audio;
pub(super) use settings::refresh_row_labels;

/// 将各业务域的 Slint 回调一次性绑定到主窗口；闭包捕获的状态句柄决定后续配置与引擎更新落点。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    // 先绑定路由启停，再绑定播放器与设置入口；所有域共享同一 AppState 快照。
    route::register(ui, state);
    route_start::register(ui, state);
    library::register(ui, state);
    library_entries::register(ui, state);
    player::register(ui, state);
    library_view::register(ui, state);
    settings::register(ui, state);
}

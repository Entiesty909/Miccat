//! Slint 回调按业务域注册。

mod library;
mod library_entries;
mod library_view;
mod player;
mod route;
mod route_start;
mod settings;

use super::{AppState, MainWindow};
pub(super) use settings::refresh_row_labels;

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    route::register(ui, state);
    route_start::register(ui, state);
    library::register(ui, state);
    library_entries::register(ui, state);
    player::register(ui, state);
    library_view::register(ui, state);
    settings::register(ui, state);
}

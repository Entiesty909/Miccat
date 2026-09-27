//! Slint 回调按业务域注册。

mod library;
mod library_entries;
mod library_metadata;
mod player;
mod route;
mod route_start;
mod settings;

use super::{AppState, MainWindow};

pub(super) fn register(ui: &MainWindow, state: &AppState) {
    route::register(ui, state);
    route_start::register(ui, state);
    library::register(ui, state);
    library_entries::register(ui, state);
    library_metadata::register(ui, state);
    player::register(ui, state);
    settings::register(ui, state);
}

//! 诊断中心与 Slint 的桥接：同步快照、面板开关、复制与清空。
//!
//! 面板只读 `diagnostics` 的快照；开关、清空、移除、复制等动作写回诊断中心，
//! 由下一次 [`sync`] 把结果灌回 Slint 全局 `DiagState`。调试开关的改动立即生效并持久化。

use super::*;
use crate::diagnostics::{self, DebugLevel, Diagnostic, Severity};
use std::cell::Cell;

thread_local! {
    /// 上次写入 Slint 的快照指纹；内容未变时跳过模型重写。
    static LAST_SYNC: Cell<(usize, usize, u64, u64)> = const { Cell::new((usize::MAX, 0, 0, 0)) };
}

/// 把诊断快照写入 Slint 全局状态；未读数变化时更新徽标。
pub(super) fn sync(ui: &MainWindow, last_unseen: &Cell<usize>) {
    let hub = diagnostics::hub();
    let snapshot = hub.snapshot();
    let unseen = hub.unseen();
    // 快照最新在前：首项最新，末项最旧；两者加条数与未读数足以刻画一次变更。
    let newest = snapshot.first().map(|item| item.id).unwrap_or(0);
    let oldest = snapshot.last().map(|item| item.id).unwrap_or(0);
    let signature = (snapshot.len(), unseen, newest, oldest);
    let changed = LAST_SYNC.with(|cache| cache.replace(signature) != signature);
    if changed {
        fill(ui, &snapshot);
    }
    let state = ui.global::<DiagState>();
    // 徽标只提示未读数量；面板开关由用户决定，这里不自动弹出。
    if unseen != last_unseen.get() {
        last_unseen.set(unseen);
        state.set_unseen(unseen as i32);
    }
    if state.get_selected() >= snapshot.len() as i32 {
        state.set_selected(-1);
    }
}

/// 把一条快照整体灌入面板的各个模型。
fn fill(ui: &MainWindow, snapshot: &[Diagnostic]) {
    let state = ui.global::<DiagState>();
    let severities: Vec<i32> = snapshot.iter().map(|item| item.severity.code()).collect();
    state.set_count(snapshot.len() as i32);
    state.set_severities(ModelRc::new(VecModel::from(severities)));
    state.set_titles(string_model(
        snapshot.iter().map(|item| item.title.replace('\n', " ")),
    ));
    state.set_sources(string_model(
        snapshot.iter().map(|item| item.source.clone()),
    ));
    state.set_times(string_model(snapshot.iter().map(|item| item.time.clone())));
    state.set_details(string_model(
        snapshot.iter().map(|item| item.detail.clone()),
    ));
    state.set_hints(string_model(snapshot.iter().map(|item| item.hint.clone())));
}

/// 把字符串迭代器包成 Slint 字符串模型。
fn string_model(items: impl Iterator<Item = String>) -> ModelRc<slint::SharedString> {
    let values: Vec<slint::SharedString> = items.map(|item| item.into()).collect();
    ModelRc::new(VecModel::from(values))
}

/// 上报一条用户可见错误：写状态栏短句并进问题中心。
pub(super) fn report_error(ui: &MainWindow, error: &crate::error::Error) {
    let message = error.user_message();
    ui.set_error_text(message.as_str().into());
    let detail = format!("{}: {}", error.code(), error.detail());
    diagnostics::hub().report(Severity::Error, "app", &message, &detail, error.hint());
}

/// 上报一条警告短句；只进问题中心，不占用状态栏短句。
pub(super) fn report_warning(_ui: &MainWindow, title: &str) {
    diagnostics::hub().report(Severity::Warn, "app", title, "", "");
}

/// 上报一条信息短句；只进问题中心，不占用状态栏短句。
pub(super) fn report_info(_ui: &MainWindow, title: &str) {
    diagnostics::hub().report(Severity::Info, "app", title, "", "");
}

/// 启动时把持久化的调试开关灌入 Slint 并应用到运行时过滤器。
pub(super) fn init(ui: &MainWindow, config: &config::AppConfig) {
    let master = DebugLevel::from_code(config.debug.master);
    if let Some(control) = diagnostics::control() {
        control.set_master(master);
        for (key, code) in &config.debug.categories {
            control.set_category(key, Some(DebugLevel::from_code(*code)));
        }
    }
    let state = ui.global::<DiagState>();
    state.set_debug_master(master.code());
    state.set_debug_category_labels(string_model(
        diagnostics::CATEGORIES
            .iter()
            .map(|category| category.label.to_string()),
    ));
    state.set_debug_category_levels(ModelRc::new(VecModel::from(category_levels(
        config, master,
    ))));
}

/// 计算每个分类的当前级别：显式覆盖优先，否则跟随默认级别。
fn category_levels(config: &config::AppConfig, master: DebugLevel) -> Vec<i32> {
    diagnostics::CATEGORIES
        .iter()
        .map(|category| {
            config
                .debug
                .categories
                .get(category.key)
                .map(|code| DebugLevel::from_code(*code).code())
                .unwrap_or(master.code())
        })
        .collect()
}

/// 注册诊断面板回调。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    register_panel(ui);
    register_debug(ui, state);
}

/// 注册问题面板的开关、选中、清空、移除、复制与打开日志。
fn register_panel(ui: &MainWindow) {
    let state = ui.global::<DiagState>();
    {
        let ui_weak = ui.as_weak();
        state.on_toggle(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let state = ui.global::<DiagState>();
            if state.get_open() {
                state.set_open(false);
                return;
            }
            state.set_open(true);
            diagnostics::hub().mark_seen();
            state.set_unseen(0);
        });
    }
    {
        let ui_weak = ui.as_weak();
        state.on_close(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<DiagState>().set_open(false);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        state.on_select(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<DiagState>().set_selected(index);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        state.on_clear(move || {
            diagnostics::hub().clear();
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<DiagState>().set_selected(-1);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        state.on_remove(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let state = ui.global::<DiagState>();
            if let Some(id) = selected_id(state.get_selected()) {
                diagnostics::hub().dismiss(id);
            }
            state.set_selected(-1);
        });
    }
    {
        let ui_weak = ui.as_weak();
        state.on_copy(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let state = ui.global::<DiagState>();
            let Some(text) = selected_text(state.get_selected()) else {
                return;
            };
            if let Err(message) = crate::platform::clipboard::set_text(&text) {
                ui.set_error_text(message.into());
            }
        });
    }
    state.on_open_log(open_log_in_explorer);
}

/// 注册设置页的调试开关回调，改动立即生效并持久化。
fn register_debug(ui: &MainWindow, state: &AppState) {
    let global = ui.global::<DiagState>();
    {
        let ui_weak = ui.as_weak();
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        global.on_debug_master_changed(move |index| {
            let level = DebugLevel::from_code(index);
            if let Some(control) = diagnostics::control() {
                control.set_master(level);
            }
            app_config.borrow_mut().debug.master = level.code();
            save_queue.schedule();
            if let Some(ui) = ui_weak.upgrade() {
                let config = app_config.borrow();
                ui.global::<DiagState>()
                    .set_debug_category_levels(ModelRc::new(VecModel::from(category_levels(
                        &config, level,
                    ))));
            }
        });
    }
    {
        let app_config = state.app_config.clone();
        let save_queue = state.save_queue.clone();
        global.on_debug_category_changed(move |index, level| {
            let position = usize::try_from(index).ok();
            let Some(category) = position.and_then(|i| diagnostics::CATEGORIES.get(i)) else {
                return;
            };
            let level = DebugLevel::from_code(level);
            if let Some(control) = diagnostics::control() {
                control.set_category(category.key, Some(level));
            }
            let mut config = app_config.borrow_mut();
            config
                .debug
                .categories
                .insert(category.key.to_string(), level.code());
            save_queue.schedule();
        });
    }
}

/// 取当前选中项的诊断 id；未选中或越界时为 `None`。
fn selected_id(index: i32) -> Option<u64> {
    let position = usize::try_from(index).ok()?;
    diagnostics::hub()
        .snapshot()
        .get(position)
        .map(|item| item.id)
}

/// 取当前选中项的可复制文本；未选中或越界时为 `None`。
fn selected_text(index: i32) -> Option<String> {
    let position = usize::try_from(index).ok()?;
    diagnostics::hub().snapshot().get(position).map(copy_text)
}

/// 拼出可复制的诊断文本：级别与时间、标题、详情与建议，空行省略。
fn copy_text(diag: &Diagnostic) -> String {
    let mut text = format!("[{}] {}  {}", diag.severity.label(), diag.time, diag.title);
    for extra in [&diag.detail, &diag.hint] {
        if !extra.is_empty() {
            text.push('\n');
            text.push_str(extra);
        }
    }
    text
}

/// 在资源管理器中选中诊断日志文件；打不开只记调试日志。
fn open_log_in_explorer() {
    let path = diagnostics::log_path();
    let arg = format!("/select,{}", path.display());
    if let Err(error) = std::process::Command::new("explorer.exe").arg(arg).spawn() {
        tracing::debug!("open diag log in explorer: {error}");
    }
}

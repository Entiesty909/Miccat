//! 后台枚举进程、物理麦克风和输出端点，并把同一快照安全交回 UI 线程。

use slint::ComponentHandle;
use std::sync::atomic::Ordering;
use std::thread;

use super::super::super::state::EnumeratedLists;
use super::super::{AppState, MainWindow};

/// 注册设备/进程刷新回调；通过 pending 原子标记合并重复请求，并把枚举工作移出 UI 线程。
pub(super) fn register(ui: &MainWindow, state: &AppState) {
    let ui_weak = ui.as_weak();
    let list_refresh = state.list_refresh.clone();
    // 弱引用升级失败表示窗口已销毁，此时丢弃事件而不触碰共享状态。
    ui.on_refresh(move || {
        // 回调只采集轻量筛选参数；真正的枚举在后台线程执行，避免阻塞 Slint 事件循环。
        let Some(ui) = ui_weak.upgrade() else { return };
        // 窗口关闭后弱引用无法升级，刷新请求应静默丢弃而不能继续触碰 UI。
        // 同一时间只允许一个枚举线程写入结果，避免刷新按钮制造并发设备快照。
        // pending 是合并请求的闸门：已有线程未完成时，后续刷新无需再创建线程。
        if list_refresh
            .pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        // 在 UI 线程读取筛选开关，再把不可变值传入后台线程，避免跨线程访问 Slint。
        // 只跨线程传递复制出的布尔值；后台线程不读取 Slint 对象或 UI 属性。
        let include_all = ui.get_all_devices();
        let worker_state = list_refresh.clone();
        if let Err(error) = thread::Builder::new()
            // 使用命名线程便于诊断后台枚举耗时和异常，不把线程句柄带回 UI 等待。
            .name("audio-lists-refresh".into())
            .spawn(move || {
                // 线程只负责枚举并写入结果槽，UI 线程稍后消费该槽位。
                // 结果槽由互斥锁保护；线程完成后只发布一次快照，消费方负责清理 pending。
                let result = refresh_lists(include_all);
                if let Ok(mut slot) = worker_state.result.lock() {
                    // 替换而非追加结果，保证 UI 永远消费一次刷新请求对应的完整列表。
                    *slot = Some(result);
                }
            })
        {
            list_refresh.pending.store(false, Ordering::Release);
            // 创建线程失败时必须撤销 pending，否则后续刷新会被永久误判为“进行中”。
            // 错误只报告用户可理解的短文案，详细枚举错误已经在 refresh_lists 中记录。
            tracing::warn!("spawn audio lists refresh thread: {error}");
            super::super::super::diagnostics::report_warning(&ui, "无法刷新音频设备和进程列表");
        }
    });
}

/// 同步枚举进程、物理麦克风和输出端点；错误转成可直接展示的用户文案。
fn refresh_lists(include_all: bool) -> Result<EnumeratedLists, String> {
    // 三次枚举按固定顺序执行，并在任一步失败时短路，避免拼出来源不同的混合快照。
    // 进程列表与 UI 的 process_index 同源；失败时不发布部分结果。
    // 物理麦克风 ID 供麦克风源下拉和混音配置使用，必须和进程列表属于同一刷新批次。
    // 输出端点是否包含全部设备由 UI 快照决定；这里返回稳定 render ID，UI 不直接持有设备对象。
    // 只有三类数据都成功才发布 EnumeratedLists，调用方可据此原子替换缓存并重建下标。
    // 三类列表必须来自同一次刷新请求，调用方才能按同一设备快照计算索引。
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

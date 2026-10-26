//! 打开一个或多个 WASAPI render 目标并装配 fanout/SFX 生命周期。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::thread;

use crate::audio::buffer::SpscRing;
use crate::audio::com;
use crate::audio::render::open_render_device;
use crate::error::Error;

use super::fanout::{FanoutTarget, run_fanout, spawn_target_render};
use super::{AudioEngine, EngineStatus, OutputState, SourceOutputRoutes, TargetState};

/// 同一虚拟驱动的两个渲染端点不能同时以共享模式打开。
///
/// 实测 VB-Cable 的「CABLE虚拟扬声器」与「CABLE In 16ch」第二个会返回
/// `AUDCLNT_E_DEVICE_IN_USE`；物理设备的接口名互不相同，不会被误判。
///
/// 这里提前拦截并给出可读原因，避免用户只看到裸 HRESULT。
/// 检查虚拟驱动族是否被重复选择；同族端点共享独占资源，必须在打开设备前拒绝。
fn same_driver_conflict(device_ids: &[String]) -> Option<String> {
    // 枚举只是友好提示；未知设备仍交给后续 WASAPI 打开流程报告真实错误。
    let devices = crate::audio::device::list_destinations(true).ok()?;
    // 记录已选虚拟驱动的标签与族名，族名相同即表示共享同一套端点资源。
    let mut seen: Vec<(&str, &str)> = Vec::new();
    // 按用户传入顺序检查，返回的首个重复族能稳定指出应替换哪一路。
    for id in device_ids {
        let Some(device) = devices.iter().find(|device| device.render_id == *id) else {
            continue;
        };
        // 物理设备没有虚拟驱动族约束，直接跳过冲突检查。
        if !device.virtual_driver {
            continue;
        }
        // 同族端点不能并行打开，必须在创建 WASAPI client 前给出可读错误。
        if let Some((first_label, _)) = seen
            .iter()
            .find(|(_, family)| *family == device.driver_family)
        {
            return Some(format!(
                "「{first_label}」和「{}」是同一只虚拟设备的两端，不能同时输出。\n\
                 请把其中一路改到别的设备（或只保留一路）。",
                device.label
            ));
        }
        // 只保存虚拟设备；下次遇到同一族即可命中上面的冲突分支。
        seen.push((device.label.as_str(), device.driver_family.as_str()));
    }
    None
}

/// 本机监听只允许物理系统默认播放端，避免把麦克风直接送回虚拟设备。
pub(super) fn physical_default_render_id() -> Option<String> {
    let id = crate::audio::device::default_render_id().ok()?;
    let destinations = crate::audio::device::list_destinations(true).ok()?;
    if destinations.iter().any(|device| {
        device.capture_id.is_none() && device.render_id == id && !device.virtual_driver
    }) {
        Some(id)
    } else {
        tracing::warn!("没有可用的物理系统默认播放端，已禁用麦克风本机试听");
        None
    }
}

/// 只在建图时计算一次掩码；越界目标索引不会匹配任何实际目标。
fn source_target_mask(
    routes: Option<&SourceOutputRoutes>,
    target_index: usize,
) -> (Vec<bool>, Vec<bool>, bool) {
    let Some(routes) = routes else {
        return (
            vec![true; super::PROCESS_SLOTS],
            vec![true; super::SFX_SLOTS],
            true,
        );
    };
    (
        (0..super::PROCESS_SLOTS)
            .map(|slot| {
                routes
                    .process
                    .get(slot)
                    .is_some_and(|indices| indices.contains(&target_index))
            })
            .collect(),
        (0..super::SFX_SLOTS)
            .map(|slot| {
                routes
                    .file
                    .get(slot)
                    .is_some_and(|indices| indices.contains(&target_index))
            })
            .collect(),
        routes.mic.contains(&target_index),
    )
}

/// AudioEngine 的输出装配与路由入口。
impl AudioEngine {
    /// 兼容旧调用方的单目标入口，实际装配仍统一走多目标路径。
    pub fn open_output(&self, device_id: &str) -> Result<(), Error> {
        self.open_outputs(&[device_id.to_owned()])
    }

    /// 打开所有指定 render 目标，并让未指定路由的源复制到每个目标。
    pub fn open_outputs(&self, device_ids: &[String]) -> Result<(), Error> {
        self.open_outputs_inner(device_ids, None)
    }

    /// 按目标索引绑定进程、音频文件和物理麦，避免一张卡片影响其他目标。
    pub fn open_routed_outputs(
        &self,
        device_ids: &[String],
        routes: SourceOutputRoutes,
    ) -> Result<(), Error> {
        self.open_outputs_inner(device_ids, Some(routes))
    }

    /// 按停止旧线程、创建新图、提交共享状态的顺序重建输出生命周期。
    fn open_outputs_inner(
        &self,
        device_ids: &[String],
        routes: Option<SourceOutputRoutes>,
    ) -> Result<(), Error> {
        // 先归还空闲试听设备，再关闭旧路由；否则驱动可能拒绝第二次打开同一端点。
        self.mic_preview.pause();
        self.stop_output_graph();
        let result = self.build_outputs_inner(device_ids, routes);
        if result.is_err() {
            self.mic_preview.resume();
        }
        result
    }

    fn build_outputs_inner(
        &self,
        device_ids: &[String],
        routes: Option<SourceOutputRoutes>,
    ) -> Result<(), Error> {
        // 空设备列表不能形成有效输出图，必须在访问 clients[0] 前返回错误。
        if device_ids.is_empty() {
            return Err(Error::RenderInit("no output devices"));
        }
        // 先做虚拟驱动族校验，再创建任一 client，避免半成品输出图。
        if let Some(conflict) = same_driver_conflict(device_ids) {
            return Err(Error::DeviceBusy(conflict));
        }

        // canonical 格式来自首个目标；其他目标在 fanout 线程中独立转换。
        // COM 只需在当前装配线程初始化；打开出来的 client 随后交给各自 render 线程使用。
        let _com = com::init_mta()?;
        // 同一端点已在转发图中时直接混入该 render 线程，不再打开第二个 WASAPI client。
        let default_id = physical_default_render_id();
        let routed_default = default_id
            .as_ref()
            .and_then(|id| device_ids.iter().position(|route| route == id));
        // 预留目标数量，避免逐个打开设备时反复扩容 client 所有权表。
        let mut clients = Vec::with_capacity(device_ids.len());
        for id in device_ids {
            // 每个 ID 独立打开；失败立即停止装配，不把部分目标发布给引擎。
            clients.push(
                open_render_device(id)
                    .map_err(|error| crate::audio::render::describe_open_failure(id, error))?,
            );
        }
        // 不在转发图中的默认播放端只承载本机麦克风试听；驱动拒绝额外 client
        // 时保留原有转发图，不让试听设备故障阻断用户的其他音源。
        let local_index = if routed_default.is_none() {
            if let Some(id) = default_id.as_deref() {
                match open_render_device(id) {
                    Ok(client) => {
                        clients.push(client);
                        Some(clients.len() - 1)
                    }
                    Err(error) => {
                        tracing::warn!("麦克风本机试听设备打开失败（{id}）：{error}");
                        None
                    }
                }
            } else {
                tracing::warn!("麦克风本机试听：取不到系统默认播放设备");
                None
            }
        } else {
            None
        };
        let preview_index = routed_default.or(local_index);
        // 所有源先混到首个目标的 canonical 格式，再由 fanout 转成各目标格式。
        let canonical = clients[0].format;
        // WASAPI mix format 供源端转换使用，不能用目标的可播放格式替代。
        let mix_format = *clients[0].mix_format();
        let capacity =
            ((canonical.sample_rate as usize).saturating_mul(canonical.channels as usize) / 2)
                .max(8192);
        // 首个目标的 canonical 环容量作为三类源环的统一基准，避免频繁扩容。
        // 三类源环都以 canonical 格式承载，容量约为半秒采样数据并设最小下限。
        let process_rings: Vec<Arc<SpscRing>> = (0..super::PROCESS_SLOTS)
            .map(|_| Arc::new(SpscRing::with_capacity_samples(capacity)))
            .collect();
        let mic_ring = Arc::new(SpscRing::with_capacity_samples(capacity));
        let sfx_rings: Vec<Arc<SpscRing>> = (0..super::SFX_SLOTS)
            .map(|_| Arc::new(SpscRing::with_capacity_samples(capacity)))
            .collect();
        // 同一停止标志广播给 fanout 与全部目标 render 线程；重建时由 stop 发布退出。
        let stop = Arc::new(AtomicBool::new(false));
        // 音量原子由 render 线程无锁读取，克隆句柄不会复制或重置卡片状态。
        let process_volumes = self.process_volumes.clone();
        let sfx_volumes = self.sfx_volumes.clone();
        let mut targets = Vec::with_capacity(clients.len());

        // 每个目标拥有独立的目标格式环，render 线程不会共享写入者。
        // 一个目标对应一套转换后的环，保证各 render 线程拥有独立写入者和设备时钟。
        for (target_index, client) in clients.into_iter().enumerate() {
            let target = client.format;
            // 目标环按设备自己的采样率、声道数分配，避免向设备写入错误帧宽度。
            let target_capacity =
                ((target.sample_rate as usize).saturating_mul(target.channels as usize) / 2)
                    .max(8192);
            // 这些环接收 fanout 已转换的数据；源环仍只由 fanout 读取。
            let target_process: Vec<Arc<SpscRing>> =
                (0..if local_index == Some(target_index) { 0 } else { super::PROCESS_SLOTS })
                .map(|_| Arc::new(SpscRing::with_capacity_samples(target_capacity)))
                .collect();
            // 物理麦在每个目标保留独立环，路由关闭时不会消费或清空其他目标。
            let target_mic = Arc::new(SpscRing::with_capacity_samples(target_capacity));
            let preview_mic = (preview_index == Some(target_index))
                .then(|| Arc::new(SpscRing::with_capacity_samples(target_capacity)));
            // 每个 SFX 槽独立转发，停止一张音频卡片不会影响其余槽位。
            let target_sfx: Vec<Arc<SpscRing>> =
                (0..if local_index == Some(target_index) { 0 } else { super::SFX_SLOTS })
                .map(|_| Arc::new(SpscRing::with_capacity_samples(target_capacity)))
                .collect();
            // 每个目标都计算独立的路由布尔向量，fanout 只据此决定写入。
            // route 掩码只决定 fanout 是否写入目标环，不改变源环或其他目标的状态。
            let (process_enabled, file_enabled, mic_enabled) = if local_index == Some(target_index) {
                (vec![false; super::PROCESS_SLOTS], vec![false; super::SFX_SLOTS], false)
            } else {
                source_target_mask(routes.as_ref(), target_index)
            };
            // 每个目标只接收自己的路由掩码；未指定路由时保持兼容行为，全部源均启用。
            // 每个目标线程独立报告设备错误；共享状态是 UI 读取的唯一错误来源。
            // render 线程持有设备 client 与目标环，独立处理阻塞写入和设备断开状态。
            let render = spawn_target_render(
                self,
                client,
                target_process.clone(),
                target_mic.clone(),
                preview_mic.clone(),
                local_index != Some(target_index),
                target_sfx.clone(),
                process_volumes.clone(),
                sfx_volumes.clone(),
                stop.clone(),
                self.status.clone(),
                self.running.clone(),
            )?;
            targets.push(TargetState {
                // 目标原生格式，render 线程按此格式提交音频帧。
                format: target,
                // 各进程槽的目标格式环，与 process_enabled 一一对应。
                process_rings: target_process,
                // 物理麦的目标格式环；是否写入由 mic_enabled 决定。
                mic_ring: target_mic,
                mic_preview_ring: preview_mic,
                // 各 SFX 槽的目标格式环，与 sfx_enabled 一一对应。
                sfx_rings: target_sfx,
                // 每个进程槽的目标开关，fanout 据此决定是否复制该槽。
                process_enabled,
                // 物理麦目标开关。
                mic_enabled,
                // 每个 SFX 槽的目标开关，保持卡片间启停互不影响。
                sfx_enabled: file_enabled,
                // render 线程句柄由 OutputState 持有，停止时先发信号再 join。
                render,
            });
        }

        // 目标状态被复制成 fanout 视图；转换器不能跨目标共享。
        // fanout 只持有目标视图；真正的 TargetState 仍由 OutputState 保管，停止时可统一 join。
        let fanout_targets = targets
            .iter()
            .map(|target| FanoutTarget::new(target, canonical))
            .collect::<Vec<_>>();
        // fanout 使用同一停止标志，收到 stop 后退出循环并由提交方 join。
        let fanout_stop = stop.clone();
        // 克隆源环的 Arc 只增加共享所有权，不复制音频数据或改变生产者位置。
        let process_source = process_rings.clone();
        let mic_source = mic_ring.clone();
        let mic_active = self.mic_active.clone();
        let mic_preview_volume = self.mic_preview_volume.clone();
        let sfx_source = sfx_rings.clone();
        // fanout 只负责分发，目标 render 的阻塞和设备时钟由各自线程处理。
        // fanout 是唯一向目标环分发的线程，render 线程只消费自己的目标环。
        let fanout = thread::Builder::new()
            .name("audio-fanout".into())
            // 在线程闭包内移动所有环和停止标志，确保线程退出前这些对象始终有效。
            .spawn(move || {
                run_fanout(
                    process_source,
                    mic_source,
                    sfx_source,
                    mic_active,
                    mic_preview_volume,
                    fanout_targets,
                    fanout_stop,
                    canonical,
                )
            })
            .map_err(|_| Error::RenderInit("spawn output fanout"))?;

        // SFX mixer 的命令通道与输出图同时建立，避免播放命令落到旧 mixer。
        // SFX 命令通道和 mixer 状态与输出图一同创建，关闭输出时统一回收。
        let (sfx_tx, sfx_rx) = crate::audio::sfx::channel();
        // SFX mixer 使用独立停止标志；它与 fanout 的停止广播分开，便于单独回收。
        let sfx_stop = Arc::new(AtomicBool::new(false));
        // 线程拿到停止标志的克隆，所有权转移后主线程仍可在停止路径发布退出。
        let sfx_stop_thread = sfx_stop.clone();
        let sfx_rings_thread = sfx_rings.clone();
        // 峰值以 f32 位模式存入原子，mixer 写入、UI 读取无需共享锁。
        let sfx_peak = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let sfx_peak_thread = sfx_peak.clone();
        // 每个 SFX 槽一个状态原子，停止或结束单槽时不会覆盖其他卡片状态。
        let sfx_states = Arc::new(
            (0..super::SFX_SLOTS)
                .map(|_| AtomicU8::new(0))
                .collect::<Vec<_>>(),
        );
        let sfx_states_thread = sfx_states.clone();
        // 每个槽位独立发布已播放位置，UI 读取不会阻塞实时混音线程。
        let sfx_positions = Arc::new(
            (0..super::SFX_SLOTS)
                .map(|_| std::sync::atomic::AtomicU64::new(0))
                .collect::<Vec<_>>(),
        );
        let sfx_positions_thread = sfx_positions.clone();
        // mixer 只消费命令通道，SFX 槽状态通过原子数组提供给 UI 轮询。
        // mixer 在线程内消费命令并写入各槽环；句柄保存到 sources 后才能被 stop 回收。
        let sfx_thread = thread::Builder::new()
            .name("sfx-mixer".into())
            .spawn(move || {
                crate::audio::sfx::run_sfx_loop(
                    sfx_rings_thread,
                    canonical,
                    sfx_stop_thread,
                    sfx_peak_thread,
                    sfx_states_thread,
                    sfx_positions_thread,
                    sfx_rx,
                )
            })
            .map_err(|_| Error::RenderInit("spawn sfx thread"))?;

        // 先提交完整 OutputState，使源路由能拿到输入环；随后提交 SFX 发送端和线程句柄。
        // 先发布输出图，再发布 SFX 通道；调用方随后才能启动源或文件播放。
        if let Ok(mut output) = self.output.lock() {
            *output = Some(OutputState {
                // fanout/render 共用的停止标志；stop 发布后先 join fanout，再 join 各目标 render。
                stop,
                // canonical 格式下的进程源环，供 fanout 读取。
                process_rings,
                // canonical 格式下的麦克风源环，供 fanout 读取。
                mic_ring,
                // fanout 读取源环时采用的统一格式。
                render_format: canonical,
                // 首个目标的 WASAPI 混音格式，供后续源转换保持一致。
                mix_format,
                // fanout 线程句柄，停止输出时必须与目标线程一起回收。
                fanout,
                // 每个目标的格式环、路由掩码和 render 线程句柄。
                targets,
            });
        }
        // sources 提交必须晚于 output 提交，保证文件播放拿到的是同一张新输出图。
        if let Ok(mut sources) = self.sources.lock() {
            // 发送端发布后，文件播放 API 才能向新 mixer 投递命令。
            sources.sfx_tx = Some(sfx_tx);
            // mixer 的停止标志由 stop_file/stop_sfx_source 取出并发布。
            sources.sfx_stop = Some(sfx_stop);
            // 最近块峰值供 UI 读取，生命周期与 mixer 绑定。
            sources.sfx_peak = Some(sfx_peak);
            // 槽状态原子供 UI 轮询，清理时先复位再停止 mixer。
            // 进度原子与状态原子共用槽位协议，生命周期随 mixer 结束。
            sources.sfx_positions = Some(sfx_positions);
            sources.sfx_states = Some(sfx_states);
            // mixer 句柄由 sources 持有，停止时发布标志后 join，避免线程泄漏。
            sources.sfx = Some(sfx_thread);
        }
        // 所有共享句柄已发布后再提交 Running；此顺序避免读者看到“运行中”却拿不到输出图。
        self.set_status(EngineStatus::Running);
        // Release 发布完成状态，状态查询端以对应的 Acquire 读取运行标志。
        self.running.store(true, Ordering::Release);
        self.route_active.store(true, Ordering::Release);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_routes_enable_every_selected_target_without_cross_routing() {
        let routes = SourceOutputRoutes {
            process: vec![vec![0, 2], vec![1], vec![], vec![usize::MAX, 7]],
            file: vec![vec![1, 2], vec![], vec![0], vec![7]],
            mic: vec![0, 2, usize::MAX],
        };

        let (process, file, mic) = source_target_mask(Some(&routes), 0);
        assert_eq!(process, [true, false, false, false]);
        assert_eq!(file, [false, false, true, false, false]);
        assert!(mic);

        let (process, file, mic) = source_target_mask(Some(&routes), 1);
        assert_eq!(process, [false, true, false, false]);
        assert_eq!(file, [true, false, false, false, false]);
        assert!(!mic);

        let (process, file, mic) = source_target_mask(Some(&routes), 2);
        assert_eq!(process, [true, false, false, false]);
        assert_eq!(file, [true, false, false, false, false]);
        assert!(mic);
    }

    #[test]
    fn no_custom_routes_broadcasts_every_source_including_preview() {
        let (process, file, mic) = source_target_mask(None, 1);
        assert_eq!(process, [true; super::super::PROCESS_SLOTS]);
        assert_eq!(file, [true; super::super::SFX_SLOTS]);
        assert!(mic);
    }
}

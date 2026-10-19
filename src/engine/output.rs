//! 打开一个或多个 WASAPI render 目标并装配 fanout/SFX 生命周期。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;

use crate::audio::buffer::SpscRing;
use crate::audio::com;
use crate::audio::render::open_render_device;
use crate::error::Error;

use super::fanout::{FanoutTarget, run_fanout, spawn_target_render};
use super::{AudioEngine, EngineStatus, OutputState, SourceOutputRoutes, TargetState, copy_wave};

/// 同一虚拟驱动的两个渲染端点不能同时以共享模式打开。
///
/// 实测 VB-Cable 的「CABLE虚拟扬声器」与「CABLE In 16ch」第二个会返回
/// `AUDCLNT_E_DEVICE_IN_USE`；物理设备的接口名互不相同，不会被误判。
///
/// 这里提前拦截并给出可读原因，避免用户只看到裸 HRESULT。
fn same_driver_conflict(device_ids: &[String]) -> Option<String> {
    let devices = crate::audio::device::list_destinations(true).ok()?;
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for id in device_ids {
        let Some(device) = devices.iter().find(|device| device.render_id == *id) else {
            continue;
        };
        if !device.virtual_driver {
            continue;
        }
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
        seen.push((device.label.as_str(), device.driver_family.as_str()));
    }
    None
}

impl AudioEngine {
    /// 单输出兼容入口。
    pub fn open_output(&self, device_id: &str) -> Result<(), Error> {
        self.open_outputs(&[device_id.to_owned()])
    }

    /// 打开一个或多个 render 目标，将全部输入送入每个目标。
    pub fn open_outputs(&self, device_ids: &[String]) -> Result<(), Error> {
        self.open_outputs_inner(device_ids, None)
    }

    /// 为进程、音频文件和物理麦配置各自的 render 目标。
    pub fn open_routed_outputs(
        &self,
        device_ids: &[String],
        routes: SourceOutputRoutes,
    ) -> Result<(), Error> {
        self.open_outputs_inner(device_ids, Some(routes))
    }

    fn open_outputs_inner(
        &self,
        device_ids: &[String],
        routes: Option<SourceOutputRoutes>,
    ) -> Result<(), Error> {
        self.stop();
        if device_ids.is_empty() {
            return Err(Error::RenderInit("no output devices"));
        }
        if routes.is_some_and(|routes| {
            routes.process >= device_ids.len()
                || routes.file >= device_ids.len()
                || routes.mic >= device_ids.len()
        }) {
            return Err(Error::RenderInit("source output index is invalid"));
        }
        if let Some(conflict) = same_driver_conflict(device_ids) {
            return Err(Error::DeviceBusy(conflict));
        }

        let _com = com::init_mta()?;
        let mut clients = Vec::with_capacity(device_ids.len());
        for id in device_ids {
            clients.push(
                open_render_device(id)
                    .map_err(|error| crate::audio::render::describe_open_failure(id, error))?,
            );
        }
        let canonical = clients[0].format;
        let mix_format = copy_wave(clients[0].mix_format());
        let capacity =
            ((canonical.sample_rate as usize).saturating_mul(canonical.channels as usize) / 2)
                .max(8192);
        let process_rings: Vec<Arc<SpscRing>> = (0..4)
            .map(|_| Arc::new(SpscRing::with_capacity_samples(capacity)))
            .collect();
        let mic_ring = Arc::new(SpscRing::with_capacity_samples(capacity));
        let sfx_ring = Arc::new(SpscRing::with_capacity_samples(capacity));
        let stop = Arc::new(AtomicBool::new(false));
        let mut targets = Vec::with_capacity(clients.len());

        for (target_index, client) in clients.into_iter().enumerate() {
            let target = client.format;
            let target_capacity =
                ((target.sample_rate as usize).saturating_mul(target.channels as usize) / 2)
                    .max(8192);
            let target_process: Vec<Arc<SpscRing>> = (0..4)
                .map(|_| Arc::new(SpscRing::with_capacity_samples(target_capacity)))
                .collect();
            let target_mic = Arc::new(SpscRing::with_capacity_samples(target_capacity));
            let target_sfx = Arc::new(SpscRing::with_capacity_samples(target_capacity));
            let (process_enabled, file_enabled, mic_enabled) =
                routes.map_or((true, true, true), |routes| {
                    (
                        routes.process == target_index,
                        routes.file == target_index,
                        routes.mic == target_index,
                    )
                });
            let render = spawn_target_render(
                self,
                client,
                target_process.clone(),
                target_mic.clone(),
                target_sfx.clone(),
                stop.clone(),
                self.status.clone(),
                self.running.clone(),
            )?;
            targets.push(TargetState {
                format: target,
                process_rings: target_process,
                mic_ring: target_mic,
                sfx_ring: target_sfx,
                process_enabled,
                mic_enabled,
                sfx_enabled: file_enabled,
                render,
            });
        }

        let fanout_targets = targets
            .iter()
            .map(|target| FanoutTarget::new(target, canonical))
            .collect::<Vec<_>>();
        let fanout_stop = stop.clone();
        let process_source = process_rings.clone();
        let mic_source = mic_ring.clone();
        let sfx_source = sfx_ring.clone();
        let fanout = thread::Builder::new()
            .name("audio-fanout".into())
            .spawn(move || {
                run_fanout(
                    process_source,
                    mic_source,
                    sfx_source,
                    fanout_targets,
                    fanout_stop,
                    canonical,
                )
            })
            .map_err(|_| Error::RenderInit("spawn output fanout"))?;

        let (sfx_tx, sfx_rx) = crate::audio::sfx::channel();
        let sfx_stop = Arc::new(AtomicBool::new(false));
        let sfx_stop_thread = sfx_stop.clone();
        let sfx_ring_thread = sfx_ring.clone();
        let sfx_peak = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let sfx_peak_thread = sfx_peak.clone();
        let sfx_thread = thread::Builder::new()
            .name("sfx-mixer".into())
            .spawn(move || {
                crate::audio::sfx::run_sfx_loop(
                    sfx_ring_thread,
                    canonical,
                    sfx_stop_thread,
                    sfx_peak_thread,
                    sfx_rx,
                )
            })
            .map_err(|_| Error::RenderInit("spawn sfx thread"))?;

        if let Ok(mut output) = self.output.lock() {
            *output = Some(OutputState {
                stop,
                process_rings,
                mic_ring,
                render_format: canonical,
                mix_format,
                fanout,
                targets,
            });
        }
        if let Ok(mut sources) = self.sources.lock() {
            sources.sfx_tx = Some(sfx_tx);
            sources.sfx_stop = Some(sfx_stop);
            sources.sfx_peak = Some(sfx_peak);
            sources.sfx = Some(sfx_thread);
        }
        self.set_status(EngineStatus::Running);
        self.running.store(true, Ordering::Release);
        Ok(())
    }
}

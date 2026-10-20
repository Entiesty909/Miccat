//! 独立本地预览输出：不进入虚拟麦克风，只写系统播放设备。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::audio::buffer::SpscRing;
use crate::audio::com;
use crate::audio::format::AudioFormat;
use crate::audio::render::{open_render_device, run_render_loop};
use crate::error::Error;

/// 独立本机试听输出；其 ring 和渲染线程不与虚拟麦克风路由共享状态。
pub struct PreviewEngine {
    /// 试听源写入、渲染线程读取的固定容量交错 f32 环形缓冲。
    ring: Arc<SpscRing>,
    /// 播放设备实际协商出的目标格式，重采样以此为准。
    target: AudioFormat,
    /// 请求渲染线程停止；释放时先置位再等待线程退出。
    stop_render: Arc<AtomicBool>,
    /// 以 f32 位模式原子发布给实时线程的音量，范围由 set_volume 限制为 0..=1。
    volume: Arc<AtomicU32>,
    /// 当前解码源及其停止句柄；同一时刻最多保留一个源。
    source: Mutex<Option<SourceThread>>,
    /// 渲染线程句柄，Drop 负责回收其所有权。
    render: Mutex<Option<JoinHandle<()>>>,
}

/// 后台试听源的停止标志与线程所有权；join 只能由拥有该结构的回收路径执行。
struct SourceThread {
    /// 源线程轮询的取消标志。
    stop: Arc<AtomicBool>,
    /// 源线程句柄，保证 ring 写入方最终退出。
    join: JoinHandle<()>,
}

/// 试听设备、解码源和实时渲染线程的生命周期管理。
impl PreviewEngine {
    /// 打开指定播放设备。
    pub fn open(device_id: &str) -> Result<Self, Error> {
        // 设备句柄只交给渲染线程；控制对象保留停止标志和 join 句柄用于资源回收。
        // 目标格式来自实际播放设备；容量按半秒估算并设下限，避免启动时扩容。
        let render = open_render_device(device_id)?;
        let target = render.format;
        let capacity =
            ((target.sample_rate as usize).saturating_mul(target.channels as usize) / 2).max(8192);
        let ring = Arc::new(SpscRing::with_capacity_samples(capacity));
        // ring 由单生产者源线程和单消费者渲染线程共享，固定容量保证实时路径不扩容。
        let stop_render = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU32::new(0.5f32.to_bits()));
        // 渲染线程独占设备接口，源线程只写 ring；音量通过原子位模式无锁发布。
        let render_thread = {
            let ring = ring.clone();
            let stop = stop_render.clone();
            let volume = volume.clone();
            // 线程闭包捕获 Arc，而不是借用 PreviewEngine，确保引擎可在创建后立即返回。
            thread::Builder::new()
                .name("preview-render".into())
                .spawn(move || {
                    // 渲染线程需要自己的 MTA；失败时 run_render_loop 会按设备错误路径退出。
                    let _com = com::init_mta();
                    run_render_loop(
                        render,
                        vec![ring],
                        None,
                        Vec::new(),
                        vec![volume],
                        None,
                        Vec::new(),
                        Arc::new(AtomicU32::new(1.0f32.to_bits())),
                        None,
                        stop,
                        |_| {},
                    );
                })
                // 线程启动失败时直接回传初始化错误，避免留下不可停止的半初始化引擎。
                .map_err(|_| Error::RenderInit("spawn preview render"))?
        };
        Ok(Self {
            ring,
            target,
            stop_render,
            volume,
            source: Mutex::new(None),
            render: Mutex::new(Some(render_thread)),
        })
    }

    /// 设置本地试听音量 0..=1。
    pub fn set_volume(&self, value: f32) {
        self.volume
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// 播放后台解码的有界音频块。
    pub fn play_stream(
        &self,
        info: crate::audio::decode::MediaInfo,
        rx: Receiver<crate::audio::sfx::StreamChunk>,
        looped: bool,
    ) {
        // 先同步停止并清空旧源，避免两条源同时写入同一试听槽。
        // 清空旧 ring 与停止旧源成对执行，防止旧样本在新文件开头泄漏。
        self.stop_source();
        self.ring.clear();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let ring = self.ring.clone();
        let target = self.target;
        // 源线程持有接收端并负责重采样；句柄存入 mutex 后由停止/Drop 路径回收。
        // Builder 线程拥有接收端；join 句柄随后交给 source 槽位统一管理生命周期。
        let join = thread::Builder::new()
            .name("preview-stream".into())
            .spawn(move || run_stream_source(ring, target, info, rx, looped, stop_thread))
            .ok();
        // 只有成功取得槽位锁才能登记句柄；登记失败时线程仍会自然结束但不影响设备线程。
        if let Some(join) = join
            && let Ok(mut slot) = self.source.lock()
        {
            *slot = Some(SourceThread { stop, join });
        }
    }
    /// 停止当前试听源。
    pub fn stop_source(&self) {
        // 先从槽位取出所有权，再置位取消并 join，避免锁住槽位等待线程结束。
        // 取出 Option 后释放 mutex，再等待线程退出，避免源线程回调路径形成锁等待。
        let source = self.source.lock().ok().and_then(|mut slot| slot.take());
        if let Some(source) = source {
            source.stop.store(true, Ordering::Release);
            // Release 发布取消请求；源线程在写 ring 前后都会观察该标志并退出。
            let _ = source.join.join();
        }
    }

    /// 请求停止试听源，并把等待工作移到后台回收线程。
    pub fn request_stop_source(&self) {
        // UI 关闭路径不能同步等待解码线程，因此移交句柄给一次性回收线程。
        // 关闭路径不能把 UI 线程卡在文件解码上，因此仅发布取消并异步 join。
        let source = self.source.lock().ok().and_then(|mut slot| slot.take());
        if let Some(source) = source {
            source.stop.store(true, Ordering::Release);
            thread::spawn(move || {
                let _ = source.join.join();
            });
        }
    }
}

/// 试听流源：把解码块重采样进 `ring`，积压超过 `lead` 就退让，节拍交给 render 线程。
///
/// 解码远快于实时，不按消费端节流会把环写成滚动窗口，听起来就是同一小段反复。
pub fn run_stream_source(
    ring: Arc<SpscRing>,
    target: AudioFormat,
    info: crate::audio::decode::MediaInfo,
    rx: Receiver<crate::audio::sfx::StreamChunk>,
    looped: bool,
    stop: Arc<AtomicBool>,
) {
    // 重采样步长按源/目标采样率计算，声道映射保持交错帧边界。
    let src_channels = usize::from(info.channels.max(1));
    let dst_channels = usize::from(target.channels.max(1));
    let step = f64::from(info.sample_rate) / f64::from(target.sample_rate);
    // 每次生成约 20 ms，既限制延迟也让 ring 的节拍足够平滑。
    let chunk_frames = (target.sample_rate as usize / 50).max(64);
    // 最多领先 render 端 5 块（约 100 ms）。
    let lead = chunk_frames * dst_channels * 5;
    let mut buffer = Vec::with_capacity(64_000);
    let mut out = Vec::with_capacity(chunk_frames * dst_channels);
    let mut pos = 0.0f64;
    let mut done = false;
    while !stop.load(Ordering::Acquire) {
        // 取消标志是唯一的跨线程退出协议，读取和 push_paced 都不会在取消后继续阻塞。
        // 只预取有限源样本；try_recv 不阻塞，短缺时交给下一轮继续等待。
        while buffer.len() < src_channels * 24_000 && !done {
            // 预取上限按源声道计数，避免多声道音频因样本数相同而占用更多无界内存。
            match rx.try_recv() {
                Ok(chunk) => {
                    // 每个块的 done 都是源端 EOF；保留已收到 samples，先播完再结束。
                    buffer.extend_from_slice(&chunk.samples);
                    done = chunk.done;
                }
                Err(_) => break,
            }
        }
        // EOF 时非循环源退出；尚未收到数据时短暂休眠，避免空转占满 CPU。
        // 循环源在短暂断粮时继续等待；非循环源只在确认 done 后退出，避免误截断。
        if buffer.is_empty() {
            if done && !looped {
                break;
            }
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        out.clear();
        // 用浮点游标做轻量线性比例抽样；每个目标声道都从同一源帧取值以保持同步。
        // 这里按帧抽样而非按单样本抽样，确保多声道游标永远保持帧边界。
        for _ in 0..chunk_frames {
            let frame = pos.floor() as usize;
            if frame >= buffer.len() / src_channels {
                break;
            }
            for ch in 0..dst_channels {
                out.push(buffer[frame * src_channels + ch.min(src_channels - 1)]);
            }
            pos += step;
        }
        if out.is_empty() {
            if done && !looped {
                break;
            }
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        // push_paced 会在 ring 领先过多时退让；消费节拍由实时渲染线程决定。
        // ring 满载时让出 CPU；不强行覆盖旧数据，否则试听会跳回环形缓冲窗口。
        ring.push_paced(&out, lead, &stop);
        let consumed = (pos.floor() as usize).min(buffer.len() / src_channels);
        // 只回收已完全消费的源帧，pos 保留小数部分以维持采样率转换的相位。
        if consumed > 2048 {
            // 仅丢弃已消费帧，并把游标移回新 buffer 起点，避免长期积累。
            let count = consumed * src_channels;
            buffer.drain(..count);
            pos -= consumed as f64;
        }
    }
}

/// 先停止源，再停止渲染线程；析构顺序保证没有线程继续访问 ring 或设备。
impl Drop for PreviewEngine {
    /// 先回收源线程，再停止并 join 渲染线程，确保共享 ring 和设备句柄已无人访问。
    fn drop(&mut self) {
        self.stop_source();
        // 先停源再停渲染，保证 render 线程退出前不会再有生产者访问 ring。
        self.stop_render.store(true, Ordering::Release);
        if let Ok(mut render) = self.render.lock()
            && let Some(join) = render.take()
        {
            let _ = join.join();
        }
    }
}

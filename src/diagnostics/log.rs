//! 诊断日志落盘：%APPDATA%\Miccat\logs\miccat.log，超 2 MiB 时截断重开。

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 单个日志文件超过该字节数时，下次打开先清空。
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// 日志路径；优先使用用户配置目录，环境变量缺失时退回当前目录。
pub(crate) fn path() -> std::path::PathBuf {
    // Windows 用户目录是默认持久化位置；缺失时退回相对路径仍允许启动日志。
    match std::env::var_os("APPDATA") {
        Some(base) if !base.is_empty() => PathBuf::from(base)
            .join("Miccat")
            .join("logs")
            .join("miccat.log"),
        _ => PathBuf::from("miccat.log"),
    }
}

/// 可克隆的日志写入句柄；实现 `std::io::Write`。
#[derive(Clone)]
pub(crate) struct SharedFile {
    /// 共享文件句柄；多条 tracing layer 克隆后仍串行写入同一个文件。
    file: Arc<Mutex<std::fs::File>>,
}

/// 通过互斥锁实现可克隆句柄的串行写入，避免日志行相互穿插。
impl std::io::Write for SharedFile {
    /// 将一段日志字节交给共享文件；锁中毒转换为 I/O 错误而非 panic。

    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| std::io::Error::other("log lock"))?;
        file.write(buf)
    }

    /// 刷新共享文件缓冲，确保关闭或查看日志前已提交内核写入。

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| std::io::Error::other("log lock"))?;
        file.flush()
    }
}

/// 创建日志目录并打开追加句柄；初始化阶段才执行目录与大小管理。
pub(crate) fn make_writer() -> std::io::Result<SharedFile> {
    // 先计算一次最终路径，后续目录创建、截断和打开必须针对同一文件。

    let path = path();
    // APPDATA 路径通常已经存在；只为日志子目录创建缺失层级。

    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir)?;
    }
    // 截断发生在 append 打开前，避免旧句柄继续把超限文件写大。

    truncate_if_oversized(&path)?;
    // 追加模式保留已有日志，SharedFile 再负责跨 layer 的互斥写入。

    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    Ok(SharedFile {
        file: Arc::new(Mutex::new(file)),
    })
}

/// 只在下次初始化时检查大小并截断，避免每条日志都触碰文件元数据。
fn truncate_if_oversized(path: &Path) -> std::io::Result<()> {
    // 文件不存在或元数据暂时不可读时按未超限处理，让后续 open 返回真实错误。

    let oversized = std::fs::metadata(path)
        .map(|meta| meta.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    // 忽略截断打开的次级错误：调用方随后仍会 open，并报告最终 I/O 结果。

    if oversized {
        let _ = OpenOptions::new().write(true).truncate(true).open(path);
    }
    Ok(())
}

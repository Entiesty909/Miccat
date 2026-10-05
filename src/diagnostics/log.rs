//! 诊断日志落盘：%APPDATA%\ProcessMic\logs\processmic.log，超 2 MiB 时截断重开。

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 单个日志文件超过该字节数时，下次打开先清空。
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// 日志文件路径。
pub(crate) fn path() -> std::path::PathBuf {
    match std::env::var_os("APPDATA") {
        Some(base) if !base.is_empty() => PathBuf::from(base)
            .join("ProcessMic")
            .join("logs")
            .join("processmic.log"),
        _ => PathBuf::from("processmic.log"),
    }
}

/// 可克隆的日志写入句柄；实现 `std::io::Write`。
#[derive(Clone)]
pub(crate) struct SharedFile {
    file: Arc<Mutex<std::fs::File>>,
}

impl std::io::Write for SharedFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| std::io::Error::other("log lock"))?;
        file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| std::io::Error::other("log lock"))?;
        file.flush()
    }
}

/// 打开（或创建）日志文件并返回写入句柄；目录不存在则创建。
pub(crate) fn make_writer() -> std::io::Result<SharedFile> {
    let path = path();
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir)?;
    }
    truncate_if_oversized(&path)?;
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    Ok(SharedFile {
        file: Arc::new(Mutex::new(file)),
    })
}

/// 已有文件超过上限时清空，保证日志不会无限增长。
fn truncate_if_oversized(path: &Path) -> std::io::Result<()> {
    let oversized = std::fs::metadata(path)
        .map(|meta| meta.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    if oversized {
        let _ = OpenOptions::new().write(true).truncate(true).open(path);
    }
    Ok(())
}

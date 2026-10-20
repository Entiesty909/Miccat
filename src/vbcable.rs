//! 从官网解析、下载、解压并提权运行 VB-CABLE 安装器。
//! 安装包目录会在本次安装后保留，以便后续卸载复用官方入口。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use crate::error::Error;

/// 官网页面，用来解析当前 Driver_Pack 编号。
const CABLE_PAGE: &str = "https://vb-audio.com/Cable/";
/// 解析失败时用的已知最新包（页面标注 OCT 2024 Pack45）。
const FALLBACK_ZIP: &str = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip";

/// 下载最新包、解压、以管理员运行 Setup。可能需要重启后设备才出现。
pub fn install_latest() -> Result<(), Error> {
    // 工作目录只用于本次安装包生命周期，先清理旧内容避免执行过期 Setup。
    let work = std::env::temp_dir().join("mintcat-vbcable");
    if work.exists() {
        let _ = fs::remove_dir_all(&work);
    }
    // 目录在安装后暂不删除，因为卸载入口会优先复用其中的官方 Setup。
    // 页面解析失败会在 resolve_latest_zip_url 内回退固定包，保证安装流程仍可继续。
    fs::create_dir_all(&work)?;
    let zip_path = work.join("VBCABLE.zip");
    let url = resolve_latest_zip_url();
    tracing::info!("downloading VB-CABLE from {url}");
    download(&url, &zip_path)?;
    // 解压后递归选择官方 Setup；不直接假设压缩包目录布局。
    extract_zip(&zip_path, &work)?;
    // 解压内容只在本地目录中查找，避免执行下载页面或外部路径中的同名文件。
    let setup = find_setup(&work).ok_or_else(|| {
        Error::Cable("安装包里没有找到 Setup.exe，请到 vb-audio.com 手动安装。".into())
    })?;
    tracing::info!("running {}", setup.display());
    // run_as_admin 持有并等待提权进程；返回成功才表示 Setup 进程已结束。
    run_as_admin(&setup, &[], "安装")?;
    Ok(())
}

/// 调官方卸载入口（提权）：优先 `VBCABLE_Setup_x64.exe -u`，
/// 找不到安装包时提示用户到「应用和功能」手动卸载。
///
/// 卸载完成后驱动通常要重启系统才彻底移除，调用方应提示用户重启。
pub fn uninstall() -> Result<(), Error> {
    // 卸载复用已存在的官方入口，避免自行删除驱动文件破坏系统注册状态。
    let setup = find_uninstall_setup().ok_or_else(|| {
        Error::Cable("未找到卸载程序，请在「应用和功能」中卸载 VB-CABLE。".into())
    })?;
    tracing::info!("uninstalling via {}", setup.display());
    run_as_admin(&setup, &["-u"], "卸载")?;
    // 只有官方 Setup 返回成功后才向上层报告卸载完成；设备移除仍可能等待系统重启。
    Ok(())
}

/// 找可用于卸载的官方 Setup：先看本程序解压目录，再看常见安装目录。
fn find_uninstall_setup() -> Option<PathBuf> {
    // 安装包目录是本模块留下的短期资源，不能假设每次卸载前都仍然存在。
    let work = std::env::temp_dir().join("mintcat-vbcable");
    if work.is_dir()
        && let Some(setup) = find_setup(&work)
    {
        return Some(setup);
    }
    // 临时目录不可用时继续探查标准目录，避免把卸载能力绑定到本次安装会话。
    // 官方安装器可能位于临时下载目录或标准 Program Files 目录。
    // VB-Audio 官方安装程序默认落在这两个目录。
    for dir in [
        r"C:\Program Files\VB\CABLE",
        r"C:\Program Files (x86)\VB\CABLE",
    ] {
        if let Some(setup) = find_setup(Path::new(dir)) {
            return Some(setup);
        }
    }
    None
}

/// 抓 Cable 页面里编号最大的 `VBCABLE_Driver_PackNN.zip`。
fn resolve_latest_zip_url() -> String {
    let html = match download_string(CABLE_PAGE) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("fetch cable page failed: {e}");
            return FALLBACK_ZIP.into();
        }
    };
    // 每轮从上一个匹配点继续，避免重复扫描同一段页面文本。
    // 只接受文件名紧跟数字和 .zip 的片段，防止页面中的其它数字干扰版本选择。
    let mut best = 0u32;
    let needle = "VBCABLE_Driver_Pack";
    let bytes = html.as_str();
    let mut start = 0;
    while let Some(rel) = bytes[start..].find(needle) {
        let at = start + rel + needle.len();
        let rest = &bytes[at..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if rest.starts_with(&format!("{digits}.zip"))
            && let Ok(n) = digits.parse::<u32>()
            && n > best
        {
            best = n;
        }
        start = at + 1;
        if start >= bytes.len() {
            break;
        }
    }
    // 版本号只用于选择包地址，下载仍由 curl 的成功状态和文件大小共同校验。
    // 页面没有可识别包时使用已知地址，而不是拼出不可验证的 URL。
    if best == 0 {
        FALLBACK_ZIP.into()
    } else {
        format!("https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack{best}.zip")
    }
}

/// curl 下载到文件。Win11 自带 curl。
fn download(url: &str, dest: &Path) -> Result<(), Error> {
    // curl 由独立进程执行，标准输入关闭可避免等待交互式提示。
    let status = Command::new("curl")
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(dest)
        .arg(url)
        .stdin(Stdio::null())
        .status()
        .map_err(|_| Error::Cable("本机没有 curl，无法下载 VB-CABLE。".into()))?;
    // 只把成功退出视为下载完成；curl 的重试不会替代最终状态检查。
    if !status.success() {
        return Err(Error::Cable("下载 VB-CABLE 失败，请检查网络。".into()));
    }
    // 极小文件通常是错误页或截断包，禁止把它交给 tar/Setup。
    if dest.metadata().map(|m| m.len()).unwrap_or(0) < 1024 {
        return Err(Error::Cable("下载的安装包不完整。".into()));
    }
    // 安装包由调用方持有到 Setup 结束，函数本身不删除目标文件以保留卸载入口。
    Ok(())
}

/// curl 把页面拉到内存（有上限）。
fn download_string(url: &str) -> Result<String, Error> {
    // 官网 HTML 只在内存中用于版本解析，curl 输出不会写入用户配置目录。
    let out = Command::new("curl")
        .args(["-fsSL", "--retry", "2", url])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| Error::Cable("本机没有 curl，无法下载 VB-CABLE。".into()))?;
    if !out.status.success() {
        return Err(Error::Cable("无法打开 VB-CABLE 官网。".into()));
    }
    // 页面内容只返回给版本解析逻辑，不进入安装器参数，降低路径注入风险。
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Windows 自带 tar 解 zip。
fn extract_zip(zip: &Path, dest: &Path) -> Result<(), Error> {
    // tar 在目标目录内展开，后续搜索只信任展开出的本地 Setup 路径。
    let status = Command::new("tar")
        .args(["-xf"])
        .arg(zip)
        .arg("-C")
        .arg(dest)
        .stdin(Stdio::null())
        .status()
        .map_err(|_| Error::Cable("无法解压安装包。".into()))?;
    if !status.success() {
        return Err(Error::Cable("解压 VB-CABLE 安装包失败。".into()));
    }
    // tar 返回后才继续搜索，避免在压缩包尚未展开时把缺失 Setup 误报为安装器错误。
    Ok(())
}

/// 优先 x64 Setup。
fn find_setup(root: &Path) -> Option<PathBuf> {
    // x64 安装器优先，其他 Setup 作为兼容回退；按优先级排序后取候选。
    let mut found = Vec::new();
    collect_setups(root, &mut found);
    found.sort_by_key(|p| {
        let n = p
            .file_name()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if n.contains("x64") {
            0
        } else if n.contains("setup") {
            1
        } else {
            2
        }
    });
    // 只返回一个入口，保证安装与卸载不会并行启动多个官方 Setup。
    found.into_iter().next()
}

/// 递归收集 *setup*.exe。
fn collect_setups(dir: &Path, out: &mut Vec<PathBuf>) {
    // 遍历失败的目录被视为空目录，安装入口由上层统一转换成用户错误。
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if path.is_dir() {
            collect_setups(&path, out);
            continue;
        }
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if name.ends_with(".exe") && name.contains("setup") {
            out.push(path);
        }
    }
}

/// 以 `runas` 提权运行安装/卸载程序并等待退出；`args` 传给程序（卸载传 `-u`）。
fn run_as_admin(exe: &Path, args: &[&str], action: &str) -> Result<(), Error> {
    // 宽字符串和参数缓冲区覆盖 ShellExecuteExW 调用及其返回前的读取周期。
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let params: String = args.join(" ");
    let params_wide: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(wide.as_ptr()),
        lpParameters: PCWSTR(params_wide.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: 指针仅借用本函数持有的 NUL 结尾缓冲区；调用不跨线程保存这些地址。
    // SAFETY: ShellExecuteExW 返回的进程句柄只由当前线程等待并通过 CloseHandle 释放。
    unsafe {
        ShellExecuteExW(&mut info)
            .map_err(|_| Error::Cable(format!("已取消或无法提权{action} VB-CABLE。")))?;
        // SEE_MASK_NOCLOSEPROCESS 让 info.hProcess 指向子进程句柄；
        // 无句柄时只能报告 ShellExecute 已返回。
        if !info.hProcess.is_invalid() {
            // 等待子进程结束确保安装动作完成，再关闭 Win32 返回的进程句柄。
            let wr = WaitForSingleObject(info.hProcess, INFINITE);
            let _ = CloseHandle(info.hProcess);
            if wr != WAIT_OBJECT_0 {
                return Err(Error::Cable(format!("{action}程序异常结束。")));
            }
        }
    }
    Ok(())
}

/// 以大小写不敏感的设备名启发式识别 VB-CABLE，不承诺对应具体端点。
pub fn device_looks_like_cable(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.contains("VB-AUDIO")
        || n.contains("VB AUDIO")
        || n.contains("CABLE")
        || n.contains("VIRTUAL AUDIO")
}

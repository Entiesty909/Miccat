#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::process::Command;

fn main() {
    // 仅 release 链接为 Windows 窗口子系统，避免双击 exe 弹出控制台黑框；
    // dev/debug 保留控制台，便于查看 tracing 日志。
    slint_build::compile("ui/app-window.slint").expect("compile slint");
    #[cfg(target_os = "windows")]
    embed_application_icon();
}

#[cfg(target_os = "windows")]
fn embed_application_icon() {
    println!("cargo:rerun-if-changed=resources/app.rc");
    println!("cargo:rerun-if-changed=resources/icon.ico");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let resource = out_dir.join("miccat-icon.res");
    let status = Command::new("rc.exe")
        .current_dir("resources")
        .arg("/nologo")
        .arg(format!("/fo{}", resource.display()))
        .arg("app.rc")
        .status()
        .expect("Windows Resource Compiler (rc.exe) is required to embed resources/icon.ico");

    assert!(status.success(), "failed to compile resources/app.rc");
    println!("cargo:rustc-link-arg-bin=miccat={}", resource.display());
}
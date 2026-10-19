use std::path::PathBuf;
use std::process::Command;

fn main() {
    slint_build::compile("ui/app-window.slint").expect("compile slint");
    embed_application_icon();
}

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

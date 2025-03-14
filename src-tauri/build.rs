use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    
    let mut build = cc::Build::new();
    build.cpp(true);
    build.file("src/lib/soundtouch/soundtouch_bridge.cpp");
    
    if target_os == "macos" {
        build.include("/opt/homebrew/include/soundtouch");
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
        println!("cargo:rustc-link-lib=SoundTouch");
    } else if target_os == "linux" || target_os == "freebsd" {
        build.include("/usr/include/soundtouch");
        println!("cargo:rustc-link-lib=SoundTouch");
    }
    
    build.compile("soundtouch_bridge");
    
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib/soundtouch/soundtouch_bridge.cpp");
    
    tauri_build::build()
}

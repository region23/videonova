use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Only run the custom build logic if we're not inside a docs.rs build
    if env::var("DOCS_RS").is_err() {
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-changed=src/utils/tts/soundtouch_bridge.cpp");
        
        // Check for the presence of SoundTouch library
        let mut soundtouch_found = false;
        
        // Platform-specific library detection
        if cfg!(target_os = "macos") {
            println!("cargo:rustc-link-search=native=/opt/homebrew/opt/sound-touch/lib");
            println!("cargo:rustc-link-lib=dylib=SoundTouch");
            
            // Compile the C++ bridge file
            let output = Command::new("c++")
                .args(&[
                    "-c",
                    "-o", "src/utils/tts/soundtouch_bridge.o",
                    "src/utils/tts/soundtouch_bridge.cpp",
                    "-I/opt/homebrew/opt/sound-touch/include",
                    "-std=c++11",
                    "-fPIC",
                ])
                .output()
                .expect("Failed to compile soundtouch_bridge.cpp");
            
            if !output.status.success() {
                panic!("Failed to compile soundtouch_bridge.cpp: {}", String::from_utf8_lossy(&output.stderr));
            }
            
            // Create static library
            let output = Command::new("ar")
                .args(&[
                    "crus",
                    "src/utils/tts/libsoundtouch_bridge.a",
                    "src/utils/tts/soundtouch_bridge.o"
                ])
                .output()
                .expect("Failed to create static library");
            
            if !output.status.success() {
                panic!("Failed to create static library: {}", String::from_utf8_lossy(&output.stderr));
            }

            // Link to the C++ standard library
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-search=native=src/utils/tts");
            println!("cargo:rustc-link-lib=static=soundtouch_bridge");
            soundtouch_found = true;
        } else if cfg!(target_os = "linux") {
            println!("cargo:rustc-link-lib=dylib=SoundTouch");
            
            // Try using pkg-config
            if let Ok(pkg_output) = Command::new("pkg-config")
                .args(&["--cflags", "--libs", "soundtouch"])
                .output()
            {
                if pkg_output.status.success() {
                    let flags = String::from_utf8_lossy(&pkg_output.stdout).trim().to_string();
                    // Extract include paths and library paths
                    for flag in flags.split_whitespace() {
                        if flag.starts_with("-I") {
                            // Include path
                            println!("cargo:rustc-env=CXXFLAGS={}", flag);
                        } else if flag.starts_with("-L") {
                            // Library path
                            println!("cargo:rustc-link-search={}", &flag[2..]);
                        }
                    }
                    
                    // Compile our bridge file using g++
                    let output = Command::new("g++")
                        .args(&[
                            "-std=c++11",
                            "-c",
                            "src/utils/tts/soundtouch_bridge.cpp",
                            "-o",
                            "src/utils/tts/soundtouch_bridge.o",
                            &flags,
                        ])
                        .output();
                    
                    if let Ok(out) = output {
                        if out.status.success() {
                            // Link the compiled bridge file
                            println!("cargo:rustc-link-search=native=src/utils/tts");
                            println!("cargo:rustc-link-lib=static=soundtouch_bridge");
                            
                            // Now compile the object file into a static lib
                            let ar_output = Command::new("ar")
                                .args(&["crus", "src/utils/tts/libsoundtouch_bridge.a", "src/utils/tts/soundtouch_bridge.o"])
                                .output();
                                
                            if let Ok(ar_out) = ar_output {
                                if ar_out.status.success() {
                                    println!("cargo:rustc-link-lib=SoundTouch");
                                    soundtouch_found = true;
                                }
                            }
                        }
                    }
                } else {
                    // Check common Linux paths
                    for path in &["/usr/lib", "/usr/local/lib"] {
                        let lib_path = PathBuf::from(path);
                        if lib_path.join("libSoundTouch.so").exists() {
                            println!("cargo:rustc-link-search={}", path);
                            soundtouch_found = true;
                            break;
                        }
                    }
                }
            }
        } else if cfg!(target_os = "windows") {
            // Check Windows paths
            for path in &[
                "C:\\Program Files\\SoundTouch\\lib", 
                "C:\\Program Files (x86)\\SoundTouch\\lib"
            ] {
                let lib_path = PathBuf::from(path);
                if lib_path.exists() {
                    println!("cargo:rustc-link-search={}", path);
                    soundtouch_found = true;
                    break;
                }
            }
        }
        
        // Link with SoundTouch if found
        if soundtouch_found {
            println!("cargo:rustc-link-lib=SoundTouch");
        } else {
            // Print a warning but don't fail - we'll handle the missing library at runtime
            println!("cargo:warning=SoundTouch library not found at build time. Will try to install at runtime.");
        }
        
        tauri_build::build()
    }
}

extern crate bindgen;
extern crate cc;

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=bindings.h");

    let mut builder = bindgen::Builder::default()
        .header("bindings.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .use_core()
        .derive_default(true)
        .allowlist_type("fb_var_screeninfo")
        .allowlist_type("fb_fix_screeninfo")
        .allowlist_var("FBIOGET_VSCREENINFO")
        .allowlist_var("FBIOPUT_VSCREENINFO")
        .allowlist_var("FBIOGET_FSCREENINFO")
        .allowlist_var("FB_ACTIVATE_NOW")
        .allowlist_var("FBIOBLANK")
        .allowlist_var("FB_BLANK_.*")
        .allowlist_var("KDSETMODE")
        .allowlist_var("KD_TEXT")
        .allowlist_var("KD_GRAPHICS");

    let build_helper = cc::Build::new();
    let compiler = build_helper.get_compiler();
    let compiler_path = compiler.path();

    let target = env::var("TARGET").unwrap();
    
    if target.contains("linux") {
        println!("cargo:warning=Detected compiler: {:?}", compiler_path);

        let mut sysroot_found = false;

        let sysroot_output = Command::new(compiler_path)
            .arg("-print-sysroot")
            .output();

        if let Ok(output) = sysroot_output {
            if output.status.success() {
                let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !sysroot.is_empty() {
                    println!("cargo:warning=Found sysroot via -print-sysroot: {}", sysroot);
                    builder = builder.clang_arg(format!("--sysroot={}", sysroot));
                    sysroot_found = true;
                }
            }
        }
        if !sysroot_found {
            println!("cargo:warning=Sysroot not found, falling back to header path extraction...");
            
            let output = Command::new(compiler_path)
                .args(&["-E", "-Wp,-v", "-xc", "/dev/null"])
                .output();

            if let Ok(output) = output {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let mut in_search_list = false;

                for line in stderr.lines() {
                    if line.starts_with("#include <...> search starts here:") {
                        in_search_list = true;
                        continue;
                    }
                    if line.starts_with("End of search list.") {
                        break;
                    }
                    if in_search_list {
                        let path = line.trim();
                        println!("cargo:warning=Found include path via fallback: {}", path);
                        builder = builder.clang_arg(format!("-I{}", path));
                    }
                }
            } else {
                println!("cargo:warning=Fallback extraction failed.");
            }
        }
    }

    let bindings = builder
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
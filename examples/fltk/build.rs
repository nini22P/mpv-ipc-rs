use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=fltk.manifest");

    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let manifest_path =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("fltk.manifest");
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_env == "msvc" {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest_path.display()
        );
    } else if target_env == "gnu" {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let rc_path = out_dir.join("manifest.rc");
        let obj_path = out_dir.join("manifest.o");
        let escaped = manifest_path.display().to_string().replace('\\', "\\\\");
        fs::write(&rc_path, format!("1 24 \"{}\"\n", escaped)).ok();
        if Command::new("windres")
            .args([rc_path.to_str().unwrap(), "-o", obj_path.to_str().unwrap()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            println!("cargo:rustc-link-arg-bins={}", obj_path.display());
        }
    }
}

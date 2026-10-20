use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let res_path = out_dir.join("app.res");

    // Locate rc.exe in PATH or Windows Kits
    let rc_candidates = [
        PathBuf::from("rc.exe"),
        PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\rc.exe"),
        PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\rc.exe"),
        PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22000.0\x64\rc.exe"),
    ];

    for rc in &rc_candidates {
        if rc.as_os_str() == "rc.exe" || rc.exists() {
            let status = Command::new(rc)
                .args(["/nologo", "/fo"])
                .arg(&res_path)
                .arg("app.rc")
                .status();

            if let Ok(s) = status {
                if s.success() && res_path.exists() {
                    println!("cargo:rustc-link-arg-bins={}", res_path.display());
                    return;
                }
            }
        }
    }
}

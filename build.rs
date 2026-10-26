//! Incrusta el icono y la información de versión en el .exe (recurso de Windows).
//! Usa `windres` de MinGW-w64; si no está, se compila igual pero sin icono.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows-gnu") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app_res.o");
    let status = Command::new("windres")
        .args(["--input", "assets/app.rc", "--output-format=coff", "--include-dir", "assets", "--output"])
        .arg(&out)
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", out.display()),
        _ => println!("cargo:warning=windres no disponible: el ejecutable no tendrá icono"),
    }
}

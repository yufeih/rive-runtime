use std::{env, io, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("TARGET")? != "x86_64-pc-windows-msvc" {
        return Err(io::Error::from(io::ErrorKind::Unsupported).into());
    }
    let runtime = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("lib");
    println!("cargo:rerun-if-changed=lib");
    println!("cargo:runtime_dir={}", runtime.display());
    println!("cargo:rustc-link-search=native={}", runtime.display());
    println!("cargo:rustc-link-lib=dylib=rive_native");
    Ok(())
}

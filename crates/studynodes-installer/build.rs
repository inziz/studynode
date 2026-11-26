use std::env;
use std::fs;
use std::path::PathBuf;

/// Wrap a PNG image into a standard Vista+ single-entry Windows `.ico` container.
fn wrap_png_as_ico(png: &[u8]) -> Vec<u8> {
    let (w, h) = if png.len() >= 24 && &png[0..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        (
            if width >= 256 { 0u8 } else { width as u8 },
            if height >= 256 { 0u8 } else { height as u8 },
        )
    } else {
        (0u8, 0u8)
    };

    let mut ico = Vec::with_capacity(22 + png.len());
    // ICONDIR: reserved=0, type=1 (icon), count=1
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    // ICONDIRENTRY (16 bytes)
    ico.push(w);
    ico.push(h);
    ico.push(0); // color count
    ico.push(0); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // color planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes()); // image byte size
    ico.extend_from_slice(&22u32.to_le_bytes()); // offset to PNG payload
    ico.extend_from_slice(png);
    ico
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let logo_path = manifest_dir.join("../studynodes-app/assets/logo.png");
    println!("cargo:rerun-if-changed={}", logo_path.display());

    let png_bytes = fs::read(&logo_path).expect("failed to read studynodes-app/assets/logo.png");
    let ico_bytes = wrap_png_as_ico(&png_bytes);
    fs::write(out_dir.join("studynodes.ico"), ico_bytes)
        .expect("failed to write generated studynodes.ico");

    if env::var_os("CARGO_FEATURE_EMBED_APP").is_some() {
        let app_exe = manifest_dir.join("../../target/release/studynodes.exe");
        println!("cargo:rerun-if-changed={}", app_exe.display());
        assert!(
            app_exe.is_file(),
            "embed-app feature requires {} to be built first (run `cargo build --release -p studynodes-app`)",
            app_exe.display()
        );
    }
}

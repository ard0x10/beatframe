//! Gives the exe its icon, drawn by the same code as the tray icon. The icon
//! goes into a compiled resource file (.res) that the MSVC linker takes as
//! it is, so no resource compiler is needed.

#[path = "src/icon.rs"]
mod icon;

use std::path::PathBuf;

/// The sizes Windows picks from: the list and title bars, Explorer's views,
/// the taskbar at each display scale.
const SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// Memory flags in the low half, language neutral in the high half.
const MOVEABLE_PURE_DISCARDABLE: u32 = 0x1030;

/// One resource entry: its header with numbered type and name, then the
/// data, padded to four bytes.
fn entry(out: &mut Vec<u8>, kind: u16, id: u16, flags: u32, data: &[u8]) {
    let header: [u32; 8] = [
        data.len() as u32,
        32,
        0xffff | (kind as u32) << 16,
        0xffff | (id as u32) << 16,
        0,
        flags,
        0,
        0,
    ];
    for word in header {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// An icon image as Windows stores it: a 32-bit bitmap, rows bottom to top,
/// then an all-clear 1-bit mask, since the alpha channel does the masking.
fn bitmap(size: u32) -> Vec<u8> {
    let rgba = icon::rgba(size);
    let mut out = Vec::new();
    for v in [40u32, size, size * 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    let mask_row = size.div_ceil(32) * 4;
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}

fn resource() -> Vec<u8> {
    let mut out = Vec::new();
    // A .res file opens with an empty entry.
    entry(&mut out, 0, 0, 0, &[]);

    let mut group = Vec::new();
    for v in [0u16, 1, SIZES.len() as u16] {
        group.extend_from_slice(&v.to_le_bytes());
    }
    for (i, &size) in SIZES.iter().enumerate() {
        let id = i as u16 + 1;
        let image = bitmap(size);
        entry(&mut out, RT_ICON, id, MOVEABLE_PURE_DISCARDABLE, &image);
        // 256 is written as 0 in the one-byte size fields.
        let side = if size >= 256 { 0 } else { size as u8 };
        group.extend_from_slice(&[side, side, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(image.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    // The first icon group is the one Explorer shows for the exe.
    entry(&mut out, RT_GROUP_ICON, 1, MOVEABLE_PURE_DISCARDABLE, &group);
    out
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.ends_with("windows-msvc") {
        return;
    }
    let path = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("icon.res");
    std::fs::write(&path, resource()).expect("writing the icon resource");
    println!("cargo:rustc-link-arg-bins={}", path.display());
}

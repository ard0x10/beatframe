//! The app's icon: a screen outline lit along its edges in the default
//! palette, drawn at any size so no image file ships. The tray, the settings
//! window and, through build.rs, the exe all wear it.

const JADE: [u8; 3] = [0x10, 0xb8, 0xa0];

/// Samples per pixel along each axis, to smooth the edges.
const SUBSAMPLES: u32 = 4;

/// Signed distance to a rounded rectangle centred at 0, negative inside.
fn rounded_rect(px: f32, py: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = px.abs() - (hw - r);
    let qy = py.abs() - (hh - r);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// `size` by `size` pixels of straight (not premultiplied) RGBA, rows top to
/// bottom.
pub fn rgba(size: u32) -> Vec<u8> {
    let n = size as f32;
    let (hw, hh, r) = (0.41 * n, 0.34 * n, 0.10 * n);
    // A fixed part keeps the line as solid at tray size as it has always
    // been (3.4 px at 32); the rest scales, about 10 px at 256.
    let rim = 0.03 * n + 2.4;
    let depth = 0.17 * n;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut sum = 0.0;
            for sy in 0..SUBSAMPLES {
                for sx in 0..SUBSAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SUBSAMPLES as f32 - n / 2.0;
                    let py = y as f32 + (sy as f32 + 0.5) / SUBSAMPLES as f32 - n / 2.0;
                    let d = rounded_rect(px, py, hw, hh, r);
                    let line = (rim / 2.0 + 0.5 - d.abs()).clamp(0.0, 1.0);
                    let glow = if d < 0.0 { (1.0 + d / depth).clamp(0.0, 1.0).powi(2) * 0.55 } else { 0.0 };
                    sum += line.max(glow);
                }
            }
            let a = sum / (SUBSAMPLES * SUBSAMPLES) as f32;
            out.extend_from_slice(&[JADE[0], JADE[1], JADE[2], (a * 255.0).round() as u8]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(pixels: &[u8], size: u32, x: u32, y: u32) -> u8 {
        pixels[((y * size + x) * 4 + 3) as usize]
    }

    #[test]
    fn the_outline_is_lit_and_the_middle_and_corners_are_clear() {
        for size in [16, 32, 256] {
            let p = rgba(size);
            assert_eq!(p.len(), (size * size * 4) as usize);
            let mid = size / 2;
            // The outline crosses the middle of each side.
            let left = ((0.5 - 0.41) * size as f32) as u32;
            let top = ((0.5 - 0.34) * size as f32) as u32;
            assert!(alpha(&p, size, left, mid) > 200, "{size}: left edge {}", alpha(&p, size, left, mid));
            assert!(alpha(&p, size, mid, top) > 200, "{size}: top edge {}", alpha(&p, size, mid, top));
            assert!(alpha(&p, size, size - 1 - left, mid) > 200, "{size}: right edge");
            assert!(alpha(&p, size, mid, size - 1 - top) > 200, "{size}: bottom edge");
            // The screen stays dark in the middle and the icon's corners are empty.
            assert_eq!(alpha(&p, size, mid, mid), 0, "{size}: middle");
            assert_eq!(alpha(&p, size, 0, 0), 0, "{size}: corner");
            // Above the outline nothing is drawn; at 16 px the line's soft
            // edge reaches the top row, so only the larger sizes are asked.
            if size >= 32 {
                assert_eq!(alpha(&p, size, mid, 0), 0, "{size}: above");
            }
        }
    }

    #[test]
    fn light_seeps_inward_from_the_edge() {
        let size = 256;
        let p = rgba(size);
        let left = ((0.5 - 0.41) * size as f32) as u32;
        let a = |dx: u32| alpha(&p, size, left + dx, size / 2);
        // Past the line the glow fades toward the middle, never rising again.
        let fading: Vec<u8> = (8..48).step_by(8).map(a).collect();
        assert!(fading.windows(2).all(|w| w[0] >= w[1]), "{fading:?}");
        assert!(fading[0] > 60 && *fading.last().unwrap() < fading[0], "{fading:?}");
    }
}

//! Takes the light's colors from the cover of the music that is playing.
//!
//! Only sessions that report an album count as music; a video's thumbnail
//! leaves the light on its palette.

/// Two colors as sRGB 0..1: the base for the rim and kick, the accent for the
/// snare and hi-hat.
pub type Colors = ([f32; 3], [f32; 3]);

/// Pixels below this chroma (max minus min channel) are black, white or gray
/// and do not vote.
const MIN_CHROMA: f32 = 0.2;
/// A cover with fewer colorful pixels than this share counts as colorless.
const MIN_COLORFUL: f32 = 0.05;
const BINS: usize = 36;
/// A second hue has to be this many bins (10 degrees each) from the first.
const ACCENT_MIN_BINS: usize = 5;
/// ...and carry this share of the first hue's weight.
const ACCENT_MIN_SHARE: f32 = 0.15;
/// Hue shift for the accent when the cover has only one color.
const ACCENT_SHIFT: f32 = 55.0;
/// The light is never brighter than jade, the palette chosen by eye, and never
/// dimmer than violet's base; ember's near black base would let a cover go dark.
const BRIGHTEST: u32 = 0x10b8a0;
const DIMMEST: u32 = 0x7a3cff;

fn linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// WCAG relative luminance of an sRGB color.
fn luminance(c: [f32; 3]) -> f32 {
    0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2])
}

fn hex(h: u32) -> [f32; 3] {
    [((h >> 16) & 0xff) as f32 / 255.0, ((h >> 8) & 0xff) as f32 / 255.0, (h & 0xff) as f32 / 255.0]
}

/// Hue in degrees of a color that has some chroma.
fn hue(c: [f32; 3]) -> f32 {
    let [r, g, b] = c;
    let max = r.max(g).max(b);
    let chroma = max - r.min(g).min(b);
    if max == r {
        60.0 * ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / chroma + 2.0)
    } else {
        60.0 * ((r - g) / chroma + 4.0)
    }
}

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// Keeps the hue and brings the brightness inside the palette's range: a dark
/// color loses some saturation, a bright one some value.
fn fit(hue: f32, mut s: f32, mut v: f32) -> [f32; 3] {
    let (dimmest, brightest) = (luminance(hex(DIMMEST)), luminance(hex(BRIGHTEST)));
    while luminance(hsv(hue, s, v)) < dimmest && s > 0.5 {
        s -= 0.02;
    }
    while luminance(hsv(hue, s, v)) > brightest && v > 0.3 {
        v -= 0.01;
    }
    hsv(hue, s, v)
}

#[derive(Clone, Copy, Default)]
struct Bin {
    weight: f32,
    x: f32,
    y: f32,
    saturation: f32,
}

/// Picks two colors from BGRA pixels, or `None` when the cover has too little
/// color to go by.
pub fn pick(bgra: &[u8]) -> Option<Colors> {
    let mut bins = [Bin::default(); BINS];
    let mut total = 0usize;
    let mut colorful = 0usize;
    for px in bgra.chunks_exact(4) {
        let (r, g, b) = (px[2] as f32 / 255.0, px[1] as f32 / 255.0, px[0] as f32 / 255.0);
        total += 1;
        let max = r.max(g).max(b);
        let chroma = max - r.min(g).min(b);
        if chroma < MIN_CHROMA {
            continue;
        }
        colorful += 1;
        let hue = hue([r, g, b]);
        let bin = &mut bins[(hue / (360.0 / BINS as f32)) as usize % BINS];
        let (sin, cos) = hue.to_radians().sin_cos();
        bin.weight += chroma;
        bin.x += cos * chroma;
        bin.y += sin * chroma;
        bin.saturation += chroma / max * chroma;
    }
    if total == 0 || (colorful as f32) < total as f32 * MIN_COLORFUL {
        return None;
    }

    // Each hue together with its two neighbours, so a color split across a
    // bin edge is not undercounted.
    let around = |i: usize| {
        let mut sum = Bin::default();
        for j in [i + BINS - 1, i, i + 1] {
            let b = bins[j % BINS];
            sum.weight += b.weight;
            sum.x += b.x;
            sum.y += b.y;
            sum.saturation += b.saturation;
        }
        sum
    };
    let hue_of = |b: Bin| b.y.atan2(b.x).to_degrees().rem_euclid(360.0);
    let saturation_of = |b: Bin| b.saturation / b.weight;

    let first = (0..BINS).max_by(|&a, &b| around(a).weight.total_cmp(&around(b).weight))?;
    let base = around(first);
    let distance = |i: usize| {
        let d = i.abs_diff(first);
        d.min(BINS - d)
    };
    let second = (0..BINS)
        .filter(|&i| distance(i) >= ACCENT_MIN_BINS && around(i).weight >= base.weight * ACCENT_MIN_SHARE)
        .max_by(|&a, &b| around(a).weight.total_cmp(&around(b).weight));

    let base_hue = hue_of(base);
    let (accent_hue, accent_saturation) = match second {
        Some(i) => (hue_of(around(i)), saturation_of(around(i))),
        None => (base_hue + ACCENT_SHIFT, saturation_of(base)),
    };
    Some((
        fit(base_hue, saturation_of(base).clamp(0.55, 0.9), 0.85),
        fit(accent_hue, accent_saturation.clamp(0.45, 0.85), 1.0),
    ))
}

pub use platform::spawn;

#[cfg(windows)]
mod platform {
    use std::sync::mpsc;
    use std::time::Duration;

    use windows::Foundation::TypedEventHandler;
    use windows::Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform,
        ColorManagementMode, ExifOrientationMode,
    };
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
    };

    use super::{Colors, pick};

    fn hex_of(c: [f32; 3]) -> String {
        let [r, g, b] = c.map(|v| (v * 255.0).round() as u8);
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// Players often send the title first and the cover a moment later.
    const SETTLE: Duration = Duration::from_millis(300);
    const SIZE: u32 = 48;

    /// Watches the media session Windows calls current and reports its colors
    /// whenever they change; `None` means the palette should be used.
    pub fn spawn(send: impl Fn(Option<Colors>) + Send + 'static) {
        std::thread::Builder::new()
            .name("album".into())
            .spawn(move || {
                if let Err(e) = run(send) {
                    eprintln!("album: stopped: {e}");
                }
            })
            .expect("spawning the album thread");
    }

    fn run(send: impl Fn(Option<Colors>)) -> windows::core::Result<()> {
        let manager = Manager::RequestAsync()?.join()?;
        let (tx, rx) = mpsc::channel::<()>();
        let changed = tx.clone();
        manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = changed.send(());
            Ok(())
        }))?;

        let mut last: Option<Option<Colors>> = None;
        let mut watched: Option<(Session, i64)> = None;
        loop {
            if let Some((session, token)) = watched.take() {
                let _ = session.RemoveMediaPropertiesChanged(token);
            }
            let session = manager.GetCurrentSession().ok();
            if let Some(session) = &session {
                let changed = tx.clone();
                if let Ok(token) = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = changed.send(());
                    Ok(())
                })) {
                    watched = Some((session.clone(), token));
                }
            }

            let colors = session.as_ref().and_then(|s| {
                cover_colors(s).unwrap_or_else(|e| {
                    eprintln!("album: cannot read the cover: {e}");
                    None
                })
            });
            if last != Some(colors) {
                match colors {
                    Some((base, accent)) => eprintln!("album: cover colors {} {}", hex_of(base), hex_of(accent)),
                    None => eprintln!("album: no album cover, palette"),
                }
                send(colors);
                last = Some(colors);
            }

            if rx.recv().is_err() {
                return Ok(());
            }
            std::thread::sleep(SETTLE);
            while rx.try_recv().is_ok() {}
        }
    }

    fn cover_colors(session: &Session) -> windows::core::Result<Option<Colors>> {
        let props = session.TryGetMediaPropertiesAsync()?.join()?;
        if props.AlbumTitle()?.is_empty() {
            return Ok(None);
        }
        let Ok(thumbnail) = props.Thumbnail() else { return Ok(None) };
        let stream = thumbnail.OpenReadAsync()?.join()?;
        let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
        let transform = BitmapTransform::new()?;
        transform.SetScaledWidth(SIZE)?;
        transform.SetScaledHeight(SIZE)?;
        transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
        let pixels = decoder
            .GetPixelDataTransformedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Ignore,
                &transform,
                ExifOrientationMode::IgnoreExifOrientation,
                ColorManagementMode::DoNotColorManage,
            )?
            .join()?
            .DetachPixelData()?;
        Ok(pick(&pixels))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Colors;

    pub fn spawn(_send: impl Fn(Option<Colors>) + Send + 'static) {
        eprintln!("album: colors from the cover are not implemented on this platform yet");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(parts: &[(u32, usize)]) -> Vec<u8> {
        let mut px = Vec::new();
        for &(color, count) in parts {
            let [r, g, b] = hex(color).map(|c| (c * 255.0).round() as u8);
            for _ in 0..count {
                px.extend_from_slice(&[b, g, r, 255]);
            }
        }
        px
    }

    fn near(a: f32, b: f32, tolerance: f32) -> bool {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d) <= tolerance
    }

    #[test]
    fn luminance_matches_the_wcag_definition() {
        assert!((luminance([1.0, 1.0, 1.0]) - 1.0).abs() < 1e-6);
        assert!((luminance([0.0, 1.0, 0.0]) - 0.7152).abs() < 1e-6);
        assert!((luminance(hex(0x808080)) - 0.2159).abs() < 1e-3);
    }

    #[test]
    fn colorless_covers_keep_the_palette() {
        assert_eq!(pick(&image(&[(0x000000, 500), (0xffffff, 300), (0x777777, 200)])), None);
        // A small red logo on a black sleeve.
        assert_eq!(pick(&image(&[(0x000000, 970), (0xd01010, 30)])), None);
        assert_eq!(pick(&[]), None);
    }

    #[test]
    fn the_main_color_leads_and_a_distinct_second_one_follows() {
        let (base, accent) = pick(&image(&[(0x1030d0, 600), (0xf08020, 300), (0x000000, 100)])).unwrap();
        assert!(near(hue(base), hue(hex(0x1030d0)), 6.0), "base hue {}", hue(base));
        assert!(near(hue(accent), hue(hex(0xf08020)), 6.0), "accent hue {}", hue(accent));
    }

    #[test]
    fn a_single_color_gets_a_shifted_accent() {
        let (base, accent) = pick(&image(&[(0xc02030, 800), (0xffffff, 200)])).unwrap();
        assert!(near(hue(accent), hue(base) + ACCENT_SHIFT, 3.0), "{} vs {}", hue(accent), hue(base));
    }

    #[test]
    fn bright_and_dark_covers_stay_in_the_palette_range() {
        let range = luminance(hex(DIMMEST)) - 1e-3..=luminance(hex(BRIGHTEST)) + 1e-3;
        for cover in [0xffff00, 0x00ff40, 0x0000ff, 0x400080, 0xff0000] {
            let (base, accent) = pick(&image(&[(cover, 100)])).unwrap();
            for c in [base, accent] {
                assert!(range.contains(&luminance(c)), "{cover:06x}: luminance {} outside {range:?}", luminance(c));
            }
            assert!(near(hue(base), hue(hex(cover)), 3.0), "{cover:06x}: base hue {}", hue(base));
        }
    }
}

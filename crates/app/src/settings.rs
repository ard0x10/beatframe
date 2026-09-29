//! The settings file: plain TOML the user can edit, applied as soon as it is saved.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::monitors::PRIMARY;
use crate::overlay::Layout;
use crate::theme;

pub const PALETTE_NAMES: [&str; 3] = ["jade", "ice", "violet"];

/// The palette made of the two colors the user picked.
pub const CUSTOM: &str = "custom";

pub const DEFAULT_TOGGLE_KEY: &str = "ctrl+alt+shift+l";

/// Reads a key combination such as "ctrl+alt+shift+l". "win" names the Windows
/// key, which the hotkey library calls "super".
pub fn parse_key(text: &str) -> Option<global_hotkey::hotkey::HotKey> {
    let tokens: Vec<&str> = text
        .split('+')
        .map(|t| match t.trim().to_ascii_lowercase().as_str() {
            "win" | "windows" => "super",
            _ => t.trim(),
        })
        .collect();
    tokens.join("+").parse().ok()
}

const TEMPLATE: &str = r##"# BeatFrame settings. Changes apply as soon as this file is saved.

# Whether the light is on; the tray icon's On switch writes it here, so it is remembered
enabled = true

# Keys that switch the light on and off from any app, like "ctrl+alt+shift+l"; empty for none
toggle_key = "ctrl+alt+shift+l"

# Start when you sign in to Windows
start_with_windows = false

# How the light moves: layered, split, ripple, aurora or band
theme = "layered"

# Colors: jade, ice, violet, or custom for the two colors below
palette = "jade"

# The custom colors as #rrggbb: the first for the rim and the kick, the second for the snare and the hi-hat
custom_base = "#10b8a0"
custom_accent = "#6c8cff"

# Take the colors from the cover of the music playing; other sound keeps the palette
album_colors = false

# Frames per second while music plays: 30, 40 or 60
fps = 60

# "strips" draws only along the screen edges, "full" uses one window over the whole screen
layout = "strips"

# Monitors the light is drawn on: "primary" is the main display, the others go by
# the names the settings window lists, like ["primary", "Monitor name"]
monitors = ["primary"]

# Hide the light while a window covers the whole screen: games, full screen video.
# Only the monitor that is covered goes dark.
pause_on_fullscreen = false

# Each theme keeps its own values in its own table. Every theme has these:
#   thickness        how deep the light reaches into the screen, 0.5 to 2.0
#   brightness       how bright it gets, 0.2 to 2.0
#   kick, snare, hat whether the theme answers that drum
#   kick_strength, snare_strength, hat_strength
#                    how strongly it answers, 0 to 2
#   fade             how long a hit takes to fade, 0.3 to 3.0; smaller is sharper
#   resting          the glow between hits, 0 to 2; 0 leaves only the hits

[layered]
thickness = 1.0
brightness = 1.0
kick = true
snare = true
hat = true
kick_strength = 1.0
snare_strength = 1.0
hat_strength = 1.0
fade = 1.0
resting = 1.0
# How much the hi-hat shimmers along the outermost line, 0 to 2
shimmer = 1.0

[split]
thickness = 1.0
brightness = 1.0
kick = true
snare = true
hat = true
kick_strength = 1.0
snare_strength = 1.0
hat_strength = 1.0
fade = 1.0
resting = 1.0

[ripple]
thickness = 1.0
brightness = 1.0
kick = true
snare = true
hat = true
kick_strength = 1.0
snare_strength = 1.0
hat_strength = 1.0
fade = 1.0
resting = 1.0
# Seconds a wave takes to reach the top, 0.3 to 2.0; smaller is faster
wave_seconds = 0.8
# Length of the trail behind each wave, 0.2 to 3.0; 1.0 is the default length
tail = 1.0
# How many sparks the hi-hat throws, 0 to 2; 0 turns them off
sparks = 1.0

[aurora]
thickness = 1.0
brightness = 1.0
kick = true
snare = true
hat = true
kick_strength = 1.0
snare_strength = 1.0
hat_strength = 1.0
fade = 1.0
resting = 1.0
# How fast the curtain flows, 0.2 to 3.0
flow = 1.0
# How many folds the curtain has along an edge, 0.5 to 2.0
folds = 1.0

[band]
thickness = 1.0
brightness = 1.0
kick = true
snare = true
hat = true
kick_strength = 1.0
snare_strength = 1.0
hat_strength = 1.0
fade = 1.0
resting = 1.0
# How fast the colors go round the frame, 0 to 3; 0 keeps them still
spin = 1.0
# How much the inner edge of the band waves, 0 to 2; 0 keeps it straight
waves = 1.0
# How round the corners are, 0 to 1; 0 is square, 1 the roundest
corners = 0.35
"##;

/// The drums in the order every theme lists them.
pub const DRUMS: [&str; 3] = ["kick", "snare", "hat"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drum {
    /// Whether the theme answers this drum at all.
    pub on: bool,
    /// How strongly, as a factor on the hit's own strength.
    pub strength: f32,
}

/// What every theme lets the user shape. Each theme keeps its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// Factor on how deep the light reaches.
    pub thickness: f32,
    /// Factor on how much light there is, under the same peak cap.
    pub brightness: f32,
    /// Kick, snare and hi-hat.
    pub drums: [Drum; 3],
    /// Factor on how long a hit takes to fade.
    pub fade: f32,
    /// Factor on the glow between hits; 0 leaves only the hits.
    pub resting: f32,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            thickness: 1.0,
            brightness: 1.0,
            drums: [Drum { on: true, strength: 1.0 }; 3],
            fade: 1.0,
            resting: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayeredSettings {
    pub look: Look,
    pub shimmer: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplitSettings {
    pub look: Look,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RippleSettings {
    pub look: Look,
    pub wave_seconds: f32,
    pub tail: f32,
    pub sparks: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuroraSettings {
    pub look: Look,
    pub flow: f32,
    pub folds: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BandSettings {
    pub look: Look,
    pub spin: f32,
    pub waves: f32,
    pub corners: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub toggle_key: String,
    pub start_with_windows: bool,
    pub theme: String,
    pub palette: String,
    pub custom_base: [u8; 3],
    pub custom_accent: [u8; 3],
    pub album_colors: bool,
    pub fps: u32,
    pub layout: Layout,
    pub monitors: Vec<String>,
    pub pause_on_fullscreen: bool,
    pub layered: LayeredSettings,
    pub split: SplitSettings,
    pub ripple: RippleSettings,
    pub aurora: AuroraSettings,
    pub band: BandSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            toggle_key: DEFAULT_TOGGLE_KEY.into(),
            start_with_windows: false,
            theme: "layered".into(),
            palette: "jade".into(),
            custom_base: [0x10, 0xb8, 0xa0],
            custom_accent: [0x6c, 0x8c, 0xff],
            album_colors: false,
            fps: 60,
            layout: Layout::Strips,
            monitors: vec![PRIMARY.into()],
            pause_on_fullscreen: false,
            layered: LayeredSettings { look: Look::default(), shimmer: 1.0 },
            split: SplitSettings { look: Look::default() },
            ripple: RippleSettings { look: Look::default(), wave_seconds: 0.8, tail: 1.0, sparks: 1.0 },
            aurora: AuroraSettings { look: Look::default(), flow: 1.0, folds: 1.0 },
            band: BandSettings { look: Look::default(), spin: 1.0, waves: 1.0, corners: 0.35 },
        }
    }
}

impl Settings {
    /// The named theme's own look; an unknown name gets Layered's.
    #[cfg(test)]
    pub fn look(&self, theme: &str) -> &Look {
        match theme {
            "split" => &self.split.look,
            "ripple" => &self.ripple.look,
            "aurora" => &self.aurora.look,
            "band" => &self.band.look,
            _ => &self.layered.look,
        }
    }

    /// The named theme's own look to change; an unknown name gets Layered's.
    pub fn look_mut(&mut self, theme: &str) -> &mut Look {
        match theme {
            "split" => &mut self.split.look,
            "ripple" => &mut self.ripple.look,
            "aurora" => &mut self.aurora.look,
            "band" => &mut self.band.look,
            _ => &mut self.layered.look,
        }
    }

    /// Puts the named theme's values, and only those, back to their defaults.
    pub fn reset_theme(&mut self, theme: &str) {
        let d = Settings::default();
        match theme {
            "layered" => self.layered = d.layered,
            "split" => self.split = d.split,
            "ripple" => self.ripple = d.ripple,
            "aurora" => self.aurora = d.aurora,
            "band" => self.band = d.band,
            _ => {}
        }
    }

    fn looks(&self) -> [(&'static str, &Look); 5] {
        [
            ("layered", &self.layered.look),
            ("split", &self.split.look),
            ("ripple", &self.ripple.look),
            ("aurora", &self.aurora.look),
            ("band", &self.band.look),
        ]
    }
}

/// "#10b8a0" as its three channels.
pub fn parse_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

pub fn color_text(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// What the file may contain; anything missing keeps its default.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct File {
    enabled: Option<bool>,
    toggle_key: Option<String>,
    start_with_windows: Option<bool>,
    theme: Option<String>,
    palette: Option<String>,
    custom_base: Option<String>,
    custom_accent: Option<String>,
    album_colors: Option<bool>,
    fps: Option<u32>,
    layout: Option<String>,
    monitors: Option<Vec<String>>,
    pause_on_fullscreen: Option<bool>,
    layered: Option<ThemeFile>,
    split: Option<ThemeFile>,
    ripple: Option<ThemeFile>,
    aurora: Option<ThemeFile>,
    band: Option<ThemeFile>,
}

/// One theme's table. Every key any theme has is known here, so a key meant
/// for another theme is reported by name instead of failing the whole file.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct ThemeFile {
    thickness: Option<f32>,
    brightness: Option<f32>,
    kick: Option<bool>,
    snare: Option<bool>,
    hat: Option<bool>,
    kick_strength: Option<f32>,
    snare_strength: Option<f32>,
    hat_strength: Option<f32>,
    fade: Option<f32>,
    resting: Option<f32>,
    shimmer: Option<f32>,
    /// Split's older switch for the glow between hits: "off" reads as no resting glow.
    quiet_edge: Option<String>,
    wave_seconds: Option<f32>,
    tail: Option<f32>,
    sparks: Option<f32>,
    flow: Option<f32>,
    folds: Option<f32>,
    spin: Option<f32>,
    waves: Option<f32>,
    corners: Option<f32>,
}

impl ThemeFile {
    /// Reads the keys every theme has into `look`, and reports the theme keys
    /// that `theme` has none of.
    fn apply_look(&self, theme: &str, own: &[&str], look: &mut Look) {
        in_range(theme, "thickness", self.thickness, 0.5, 2.0, &mut look.thickness);
        in_range(theme, "brightness", self.brightness, 0.2, 2.0, &mut look.brightness);
        let drums = [(self.kick, self.kick_strength), (self.snare, self.snare_strength), (self.hat, self.hat_strength)];
        for (i, (on, strength)) in drums.into_iter().enumerate() {
            if let Some(on) = on {
                look.drums[i].on = on;
            }
            let key = ["kick_strength", "snare_strength", "hat_strength"][i];
            in_range(theme, key, strength, 0.0, 2.0, &mut look.drums[i].strength);
        }
        in_range(theme, "fade", self.fade, 0.3, 3.0, &mut look.fade);
        in_range(theme, "resting", self.resting, 0.0, 2.0, &mut look.resting);

        let given = [
            ("shimmer", self.shimmer.is_some()),
            ("quiet_edge", self.quiet_edge.is_some()),
            ("wave_seconds", self.wave_seconds.is_some()),
            ("tail", self.tail.is_some()),
            ("sparks", self.sparks.is_some()),
            ("flow", self.flow.is_some()),
            ("folds", self.folds.is_some()),
            ("spin", self.spin.is_some()),
            ("waves", self.waves.is_some()),
            ("corners", self.corners.is_some()),
        ];
        for (key, _) in given.iter().filter(|(key, set)| *set && !own.contains(key)) {
            eprintln!("settings: {theme} has no {key}, it belongs to another theme");
        }
    }
}

/// Command line values win over the file for this run only.
#[derive(Default)]
pub struct Overrides {
    pub theme: Option<String>,
    pub palette: Option<String>,
    pub fps: Option<u32>,
    pub layout: Option<Layout>,
}

pub fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("beatframe").join("settings.toml")
}

/// Writes the commented template when there is no file yet.
pub fn ensure_file(path: &Path) {
    if path.exists() {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, TEMPLATE) {
        eprintln!("settings: could not write {}: {e}", path.display());
    }
}

/// Reads the file on top of the defaults. A value that is out of range is
/// reported and left at its default; the rest still apply. A file that does not
/// parse at all yields `None`, so a half-typed edit changes nothing.
pub fn load(path: &Path, overrides: &Overrides) -> Option<Settings> {
    let mut s = Settings::default();
    let text = std::fs::read_to_string(path)
        .map_err(|e| eprintln!("settings: could not read {}: {e}", path.display()))
        .ok()?;
    let file = toml::from_str::<File>(&text)
        .map_err(|e| eprintln!("settings: {} not applied: {e}", path.display()))
        .ok()?;
    apply(&mut s, file);
    Some(with_overrides(s, overrides))
}

/// The defaults with the command line on top, for when the file cannot be read.
pub fn load_defaults(overrides: &Overrides) -> Settings {
    with_overrides(Settings::default(), overrides)
}

fn with_overrides(mut s: Settings, overrides: &Overrides) -> Settings {
    if let Some(t) = &overrides.theme {
        s.theme = t.clone();
    }
    if let Some(p) = &overrides.palette {
        s.palette = p.clone();
    }
    if let Some(f) = overrides.fps {
        s.fps = f;
    }
    if let Some(l) = overrides.layout {
        s.layout = l;
    }
    s
}

fn apply(s: &mut Settings, f: File) {
    if let Some(e) = f.enabled {
        s.enabled = e;
    }
    if let Some(k) = f.toggle_key {
        if k.trim().is_empty() || parse_key(&k).is_some() {
            s.toggle_key = k.trim().to_string();
        } else {
            eprintln!("settings: toggle_key \"{k}\" is not a key combination, like \"{DEFAULT_TOGGLE_KEY}\"");
        }
    }
    if let Some(w) = f.start_with_windows {
        s.start_with_windows = w;
    }
    if let Some(t) = f.theme {
        if theme::NAMES.contains(&t.as_str()) {
            s.theme = t;
        } else {
            eprintln!("settings: theme \"{t}\" unknown, use one of {}", theme::NAMES.join(", "));
        }
    }
    if let Some(p) = f.palette {
        if PALETTE_NAMES.contains(&p.as_str()) || p == CUSTOM {
            s.palette = p;
        } else {
            eprintln!("settings: palette \"{p}\" unknown, use one of {} or {CUSTOM}", PALETTE_NAMES.join(", "));
        }
    }
    for (key, text, into) in [("custom_base", f.custom_base, &mut s.custom_base), ("custom_accent", f.custom_accent, &mut s.custom_accent)] {
        if let Some(text) = text {
            match parse_color(&text) {
                Some(c) => *into = c,
                None => eprintln!("settings: {key} \"{text}\" is not a color, like \"#10b8a0\""),
            }
        }
    }
    if let Some(a) = f.album_colors {
        s.album_colors = a;
    }
    if let Some(fps) = f.fps {
        if (10..=240).contains(&fps) {
            s.fps = fps;
        } else {
            eprintln!("settings: fps {fps} out of range 10..=240");
        }
    }
    if let Some(l) = f.layout {
        match l.as_str() {
            "strips" => s.layout = Layout::Strips,
            "full" => s.layout = Layout::Full,
            _ => eprintln!("settings: layout \"{l}\" unknown, use strips or full"),
        }
    }
    if let Some(m) = f.monitors {
        let mut names: Vec<String> = Vec::new();
        for name in m.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        if names.is_empty() {
            eprintln!("settings: monitors lists none, the light stays on \"{PRIMARY}\"");
        } else {
            s.monitors = names;
        }
    }
    if let Some(p) = f.pause_on_fullscreen {
        s.pause_on_fullscreen = p;
    }
    if let Some(t) = f.layered {
        let l = &mut s.layered;
        t.apply_look("layered", &["shimmer"], &mut l.look);
        in_range("layered", "shimmer", t.shimmer, 0.0, 2.0, &mut l.shimmer);
    }
    if let Some(t) = f.split {
        let l = &mut s.split;
        t.apply_look("split", &["quiet_edge"], &mut l.look);
        match t.quiet_edge.as_deref() {
            // An explicit resting value wins over the older switch.
            Some("off") if t.resting.is_none() => l.look.resting = 0.0,
            Some("off" | "dim") | None => {}
            Some(q) => eprintln!("settings: split quiet_edge \"{q}\" unknown, use resting = 0 to 2 instead"),
        }
    }
    if let Some(t) = f.ripple {
        let r = &mut s.ripple;
        t.apply_look("ripple", &["wave_seconds", "tail", "sparks"], &mut r.look);
        in_range("ripple", "wave_seconds", t.wave_seconds, 0.3, 2.0, &mut r.wave_seconds);
        in_range("ripple", "tail", t.tail, 0.2, 3.0, &mut r.tail);
        in_range("ripple", "sparks", t.sparks, 0.0, 2.0, &mut r.sparks);
    }
    if let Some(t) = f.aurora {
        let a = &mut s.aurora;
        t.apply_look("aurora", &["flow", "folds"], &mut a.look);
        in_range("aurora", "flow", t.flow, 0.2, 3.0, &mut a.flow);
        in_range("aurora", "folds", t.folds, 0.5, 2.0, &mut a.folds);
    }
    if let Some(t) = f.band {
        let b = &mut s.band;
        t.apply_look("band", &["spin", "waves", "corners"], &mut b.look);
        in_range("band", "spin", t.spin, 0.0, 3.0, &mut b.spin);
        in_range("band", "waves", t.waves, 0.0, 2.0, &mut b.waves);
        in_range("band", "corners", t.corners, 0.0, 1.0, &mut b.corners);
    }
}

fn in_range(table: &str, key: &str, value: Option<f32>, min: f32, max: f32, into: &mut f32) {
    match value {
        Some(v) if (min..=max).contains(&v) => *into = v,
        Some(v) => eprintln!("settings: {table} {key} {v} out of range {min} to {max}"),
        None => {}
    }
}

/// One value to record in the file: its table (`None` for the top level), its
/// key and its TOML text.
pub type Change = (Option<&'static str>, &'static str, String);

/// The file lines that turn `old` into `new`, one per value that differs.
pub fn changes(old: &Settings, new: &Settings) -> Vec<Change> {
    let text = |s: &str| toml::Value::String(s.to_string()).to_string();
    let layout = |l: Layout| text(if l == Layout::Strips { "strips" } else { "full" });
    let float = |v: f32| format!("{v:?}");
    let list = |names: &[String]| toml::Value::Array(names.iter().map(|n| toml::Value::String(n.clone())).collect()).to_string();
    let (o, n) = (old, new);
    let mut out = Vec::new();
    let mut put = |differs: bool, table: Option<&'static str>, key: &'static str, value: String| {
        if differs {
            out.push((table, key, value));
        }
    };
    put(o.enabled != n.enabled, None, "enabled", n.enabled.to_string());
    put(o.toggle_key != n.toggle_key, None, "toggle_key", text(&n.toggle_key));
    put(o.start_with_windows != n.start_with_windows, None, "start_with_windows", n.start_with_windows.to_string());
    put(o.theme != n.theme, None, "theme", text(&n.theme));
    put(o.palette != n.palette, None, "palette", text(&n.palette));
    put(o.custom_base != n.custom_base, None, "custom_base", text(&color_text(n.custom_base)));
    put(o.custom_accent != n.custom_accent, None, "custom_accent", text(&color_text(n.custom_accent)));
    put(o.album_colors != n.album_colors, None, "album_colors", n.album_colors.to_string());
    put(o.fps != n.fps, None, "fps", n.fps.to_string());
    put(o.layout != n.layout, None, "layout", layout(n.layout));
    put(o.monitors != n.monitors, None, "monitors", list(&n.monitors));
    put(o.pause_on_fullscreen != n.pause_on_fullscreen, None, "pause_on_fullscreen", n.pause_on_fullscreen.to_string());

    for ((table, a), (_, b)) in o.looks().into_iter().zip(n.looks()) {
        let t = Some(table);
        put(a.thickness != b.thickness, t, "thickness", float(b.thickness));
        put(a.brightness != b.brightness, t, "brightness", float(b.brightness));
        for i in 0..3 {
            put(a.drums[i].on != b.drums[i].on, t, DRUMS[i], b.drums[i].on.to_string());
            let key = ["kick_strength", "snare_strength", "hat_strength"][i];
            put(a.drums[i].strength != b.drums[i].strength, t, key, float(b.drums[i].strength));
        }
        put(a.fade != b.fade, t, "fade", float(b.fade));
        put(a.resting != b.resting, t, "resting", float(b.resting));
    }
    put(o.layered.shimmer != n.layered.shimmer, Some("layered"), "shimmer", float(n.layered.shimmer));
    put(o.ripple.wave_seconds != n.ripple.wave_seconds, Some("ripple"), "wave_seconds", float(n.ripple.wave_seconds));
    put(o.ripple.tail != n.ripple.tail, Some("ripple"), "tail", float(n.ripple.tail));
    put(o.ripple.sparks != n.ripple.sparks, Some("ripple"), "sparks", float(n.ripple.sparks));
    put(o.aurora.flow != n.aurora.flow, Some("aurora"), "flow", float(n.aurora.flow));
    put(o.aurora.folds != n.aurora.folds, Some("aurora"), "folds", float(n.aurora.folds));
    put(o.band.spin != n.band.spin, Some("band"), "spin", float(n.band.spin));
    put(o.band.waves != n.band.waves, Some("band"), "waves", float(n.band.waves));
    put(o.band.corners != n.band.corners, Some("band"), "corners", float(n.band.corners));
    out
}

/// Records `changes` in the file, changing only their own lines so the user's
/// comments and layout stay as they are.
pub fn write(path: &Path, changes: &[Change]) {
    if changes.is_empty() {
        return;
    }
    ensure_file(path);
    let result = std::fs::read_to_string(path).and_then(|text| {
        let text = changes.iter().fold(text, |t, (table, key, value)| with_value(&t, *table, key, value));
        // Written beside the file and renamed over it, so a failed write
        // cannot leave half a file behind.
        let temp = path.with_extension("toml.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, path)
    });
    if let Err(e) = result {
        eprintln!("settings: could not write {}: {e}", path.display());
    }
}

/// `text` with `key` in `table` set to `value`. A missing top-level key goes
/// before the first table, a missing key in a table right under its header,
/// and a missing table at the end.
fn with_value(text: &str, table: Option<&str>, key: &str, value: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let line = format!("{key} = {value}");
    fn header(l: &str) -> Option<&str> {
        l.strip_prefix('[').and_then(|r| r.split(']').next()).map(str::trim)
    }
    let is_key = |l: &str| l.strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with('='));

    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let (mut scope, mut found, mut first_header, mut own_header) = (None, None, None, None);
    for (i, l) in lines.iter().enumerate() {
        let l = l.trim_start();
        if let Some(name) = header(l) {
            first_header.get_or_insert(i);
            if table == Some(name) {
                own_header.get_or_insert(i);
            }
            scope = Some(name);
        } else if scope == table && is_key(l) {
            found = Some(i);
            break;
        }
    }
    let mut at_end = false;
    match (found, table, first_header, own_header) {
        (Some(i), ..) => lines[i] = line,
        (None, None, Some(i), _) => {
            lines.insert(i, String::new());
            lines.insert(i, line);
        }
        (None, Some(_), _, Some(i)) => lines.insert(i + 1, line),
        (None, None, None, _) => {
            lines.push(line);
            at_end = true;
        }
        (None, Some(t), _, None) => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{t}]"));
            lines.push(line);
            at_end = true;
        }
    }
    let mut result = lines.join(newline);
    if text.ends_with('\n') || at_end {
        result.push_str(newline);
    }
    result
}

/// Calls `changed` whenever the settings file is written, renamed into place or
/// removed. The watcher stops when the returned value is dropped.
pub fn watch(path: &Path, changed: impl Fn() + Send + 'static) -> Option<notify::RecommendedWatcher> {
    use notify::{EventKind, RecursiveMode, Watcher};

    let dir = path.parent()?.to_path_buf();
    let name = path.file_name()?.to_os_string();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        // Editors often save to a temporary name and rename it over the file.
        if event.paths.iter().any(|p| p.file_name() == Some(name.as_os_str())) {
            changed();
        }
    })
    .map_err(|e| eprintln!("settings: cannot watch for changes: {e}"))
    .ok()?;
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| eprintln!("settings: cannot watch {}: {e}", dir.display()))
        .ok()?;
    Some(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        apply(&mut s, toml::from_str(text).unwrap());
        s
    }

    #[test]
    fn template_reads_back_as_the_defaults() {
        assert_eq!(parse(TEMPLATE), Settings::default());
    }

    /// The template lists every theme, and each one has a table there.
    #[test]
    fn the_template_has_a_table_for_every_theme() {
        let table: toml::Table = toml::from_str(TEMPLATE).unwrap();
        let listed = TEMPLATE.lines().find(|l| l.starts_with("# How the light moves:")).expect("the theme comment");
        for name in theme::NAMES {
            assert!(table.get(name).is_some_and(|t| t.is_table()), "no [{name}] in the template");
            assert!(listed.contains(name), "{name} is not named in \"{listed}\"");
        }
    }

    #[test]
    fn bad_values_keep_defaults_and_good_ones_apply() {
        let s = parse("palette = \"pink\"\nfps = 5\nlayout = \"full\"\n");
        assert_eq!(s, Settings { layout: Layout::Full, ..Settings::default() });
        let s = parse("palette = \"violet\"\nfps = 30\n");
        assert_eq!(s, Settings { palette: "violet".into(), fps: 30, ..Settings::default() });
        let s = parse("theme = \"none\"\n");
        assert_eq!(s, Settings::default());
        let s = parse("enabled = false\nstart_with_windows = true\n");
        assert_eq!(s, Settings { enabled: false, start_with_windows: true, ..Settings::default() });
        let s = parse("toggle_key = \"\"\n");
        assert_eq!(s, Settings { toggle_key: String::new(), ..Settings::default() });
        let s = parse("toggle_key = \"ctrl+alt+nokey\"\n");
        assert_eq!(s, Settings::default());
        let s = parse("toggle_key = \"Win+Shift+F9\"\n");
        assert_eq!(s.toggle_key, "Win+Shift+F9");
        let s = parse("album_colors = true\n");
        assert_eq!(s, Settings { album_colors: true, ..Settings::default() });
        let s = parse("monitors = [\"primary\", \" Right \", \"\", \"Right\"]\n");
        assert_eq!(s.monitors, ["primary", "Right"]);
        let s = parse("monitors = []\n");
        assert_eq!(s, Settings::default());
        let s = parse("pause_on_fullscreen = true\n");
        assert_eq!(s, Settings { pause_on_fullscreen: true, ..Settings::default() });
        let s = parse("[ripple]\nwave_seconds = 1.5\ntail = 9.0\nsparks = 0\n");
        assert_eq!((s.ripple.wave_seconds, s.ripple.tail, s.ripple.sparks), (1.5, 1.0, 0.0));
        let s = parse("palette = \"custom\"\ncustom_base = \"#FF0080\"\ncustom_accent = \"blue\"\n");
        assert_eq!((s.palette.as_str(), s.custom_base), ("custom", [0xff, 0x00, 0x80]));
        assert_eq!(s.custom_accent, Settings::default().custom_accent);
        for bad in ["", "#12345", "#1234567", "123456", "#gg0000", "#ééé"] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_theme_keeps_its_own_values() {
        let s = parse("[band]\nthickness = 1.8\nhat = false\nsnare_strength = 0.4\n[aurora]\nthickness = 9\nflow = 5\n");
        assert_eq!(s.band.look.thickness, 1.8);
        assert!(!s.band.look.drums[2].on);
        assert_eq!(s.band.look.drums[1].strength, 0.4);
        // The other themes keep theirs.
        assert_eq!(s.layered, Settings::default().layered);
        assert_eq!(s.aurora, Settings::default().aurora, "out of range values keep the default");
        // A key another theme owns is reported and does nothing.
        let s = parse("[layered]\ntail = 2.0\n");
        assert_eq!(s, Settings::default());
    }

    /// Split's older switch still reads: "off" means no glow between hits,
    /// unless the file also says how much.
    #[test]
    fn the_older_quiet_edge_reads_as_resting() {
        assert_eq!(parse("[split]\nquiet_edge = \"off\"\n").split.look.resting, 0.0);
        assert_eq!(parse("[split]\nquiet_edge = \"dim\"\n").split.look.resting, 1.0);
        assert_eq!(parse("[split]\nquiet_edge = \"off\"\nresting = 0.5\n").split.look.resting, 0.5);
        assert_eq!(parse("[split]\nquiet_edge = \"odd\"\n"), Settings::default());
    }

    #[test]
    fn reset_puts_back_one_theme_only() {
        let mut s = parse("[ripple]\ntail = 2.0\nfade = 2.0\n[band]\nspin = 2.5\n");
        s.reset_theme("ripple");
        assert_eq!(s.ripple, Settings::default().ripple);
        assert_eq!(s.band.spin, 2.5);
        // Every theme resets its own values and nobody else's.
        for name in theme::NAMES {
            let mut s = Settings::default();
            for other in theme::NAMES {
                s.look_mut(other).fade = 2.0;
            }
            s.reset_theme(name);
            for other in theme::NAMES {
                let fade = s.look(other).fade;
                assert_eq!(fade, if other == name { 1.0 } else { 2.0 }, "reset {name}, {other} has fade {fade}");
            }
        }
    }

    #[test]
    fn a_value_changes_only_its_own_line() {
        let with_enabled = |text: &str, on: bool| with_value(text, None, "enabled", &on.to_string());
        let text = "# mine\nenabled = true # note\ntheme = \"split\"\n\n[split]\nresting = 0.0\n";
        let off = with_enabled(text, false);
        assert_eq!(off, text.replace("enabled = true # note", "enabled = false"));
        assert!(!parse(&off).enabled);
        assert_eq!(parse(&off).theme, "split");
        assert!(parse(&with_enabled(&off, true)).enabled);

        // An older file without the key gets it before the first table.
        let old = "theme = \"ripple\"\r\n\r\n[ripple]\r\ntail = 2.0\r\n";
        let added = with_enabled(old, false);
        assert_eq!(added, "theme = \"ripple\"\r\n\r\nenabled = false\r\n\r\n[ripple]\r\ntail = 2.0\r\n");
        assert!(!parse(&added).enabled);
        assert_eq!(parse(&added).ripple.tail, 2.0);
        assert_eq!(with_enabled("fps = 30", false), "fps = 30\nenabled = false\n");

        // A key in a table is looked for in that table only.
        let tables = "tail = 9\n[band]\ntail = 1.5\n\n[ripple]\ntail = 2.0\n";
        let tail = with_value(tables, Some("ripple"), "tail", "0.5");
        assert_eq!(tail, tables.replace("tail = 2.0", "tail = 0.5"));
        let sparks = with_value(tables, Some("ripple"), "sparks", "0.0");
        assert_eq!(sparks, tables.replace("[ripple]\n", "[ripple]\nsparks = 0.0\n"));
        let missing = with_value("fps = 30\n", Some("split"), "resting", "0.0");
        assert_eq!(missing, "fps = 30\n\n[split]\nresting = 0.0\n");
        assert_eq!(parse(&missing).split.look.resting, 0.0);
    }

    /// Every field the window can change reaches the file and reads back,
    /// judged by the parser rather than by the writer.
    #[test]
    fn every_change_reads_back() {
        let mut changed = Settings {
            enabled: false,
            toggle_key: "Win+Shift+F9".into(),
            start_with_windows: true,
            theme: "ripple".into(),
            palette: CUSTOM.into(),
            custom_base: [1, 2, 3],
            custom_accent: [0xfe, 0xdc, 0xba],
            album_colors: true,
            fps: 30,
            layout: Layout::Full,
            monitors: vec!["Right \"quoted\"".into(), PRIMARY.into()],
            pause_on_fullscreen: true,
            ..Settings::default()
        };
        for name in theme::NAMES {
            let look = changed.look_mut(name);
            look.thickness = 1.75;
            look.brightness = 0.5;
            for d in &mut look.drums {
                d.on = !d.on;
                d.strength = 0.25;
            }
            look.fade = 2.5;
            look.resting = 1.5;
        }
        changed.layered.shimmer = 0.0;
        changed.ripple.wave_seconds = 1.25;
        changed.ripple.tail = 0.35;
        changed.ripple.sparks = 0.0;
        changed.aurora.flow = 2.75;
        changed.aurora.folds = 0.5;
        changed.band.spin = 0.0;
        changed.band.waves = 1.75;
        changed.band.corners = 0.9;

        let lines = changes(&Settings::default(), &changed);
        // 12 top level, 10 per theme, 9 theme options.
        assert_eq!(lines.len(), 12 + 10 * theme::NAMES.len() + 9, "{lines:?}");
        let written = lines.iter().fold(TEMPLATE.to_string(), |t, (table, key, value)| with_value(&t, *table, key, value));
        assert_eq!(parse(&written), changed);
        assert_eq!(written.lines().count(), TEMPLATE.lines().count(), "values were added instead of replaced");
        assert!(changes(&changed, &changed).is_empty());
        // A quote in a key name must not end the TOML string early.
        let quoted = Settings { toggle_key: "ctrl+\"".into(), ..Settings::default() };
        let text = with_value("", None, "toggle_key", &changes(&Settings::default(), &quoted)[0].2);
        assert_eq!(toml::from_str::<File>(&text).unwrap().toggle_key.as_deref(), Some("ctrl+\""));
    }

    #[test]
    fn the_default_key_is_ctrl_alt_shift_l() {
        use global_hotkey::hotkey::{Code, HotKey, Modifiers};
        let expected = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT), Code::KeyL);
        assert_eq!(parse_key(DEFAULT_TOGGLE_KEY), Some(expected));
        let win = HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::F9);
        assert_eq!(parse_key("Win + Shift + F9"), Some(win));
        assert_eq!(parse_key("ctrl+alt"), None);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(toml::from_str::<File>("colour = \"jade\"\n").is_err());
        assert!(toml::from_str::<File>("[band]\nspeed = 2\n").is_err());
    }
}

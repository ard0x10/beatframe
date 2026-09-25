//! The settings file: plain TOML the user can edit, applied as soon as it is saved.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::monitors::PRIMARY;
use crate::overlay::Layout;
use crate::theme;

pub const PALETTE_NAMES: [&str; 3] = ["jade", "ice", "violet"];

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

const TEMPLATE: &str = r#"# BeatFrame settings. Changes apply as soon as this file is saved.

# Whether the light is on; the tray icon's On switch writes it here, so it is remembered
enabled = true

# Keys that switch the light on and off from any app, like "ctrl+alt+shift+l"; empty for none
toggle_key = "ctrl+alt+shift+l"

# Start when you sign in to Windows
start_with_windows = false

# How the light moves: layered, split or ripple
theme = "layered"

# Colors: jade, ice or violet
palette = "jade"

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

[split]
# What an edge does while its drum is quiet: "dim" keeps a faint line, "off" goes dark
quiet_edge = "dim"

[ripple]
# Seconds a wave takes to reach the top, 0.3 to 2.0; smaller is faster
wave_seconds = 0.8
# Length of the trail behind each wave, 0.2 to 3.0; 1.0 is the default length
tail = 1.0
# How many sparks the hi-hat throws, 0 to 2; 0 turns them off
sparks = 1.0
"#;

/// What a Split edge does while its drum is quiet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuietEdge {
    Dim,
    Off,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplitSettings {
    pub quiet_edge: QuietEdge,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RippleSettings {
    pub wave_seconds: f32,
    pub tail: f32,
    pub sparks: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub toggle_key: String,
    pub start_with_windows: bool,
    pub theme: String,
    pub palette: String,
    pub album_colors: bool,
    pub fps: u32,
    pub layout: Layout,
    pub monitors: Vec<String>,
    pub pause_on_fullscreen: bool,
    pub split: SplitSettings,
    pub ripple: RippleSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            toggle_key: DEFAULT_TOGGLE_KEY.into(),
            start_with_windows: false,
            theme: "layered".into(),
            palette: "jade".into(),
            album_colors: false,
            fps: 60,
            layout: Layout::Strips,
            monitors: vec![PRIMARY.into()],
            pause_on_fullscreen: false,
            split: SplitSettings { quiet_edge: QuietEdge::Dim },
            ripple: RippleSettings { wave_seconds: 0.8, tail: 1.0, sparks: 1.0 },
        }
    }
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
    album_colors: Option<bool>,
    fps: Option<u32>,
    layout: Option<String>,
    monitors: Option<Vec<String>>,
    pause_on_fullscreen: Option<bool>,
    split: Option<SplitFile>,
    ripple: Option<RippleFile>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RippleFile {
    wave_seconds: Option<f32>,
    tail: Option<f32>,
    sparks: Option<f32>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct SplitFile {
    quiet_edge: Option<String>,
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
        if PALETTE_NAMES.contains(&p.as_str()) {
            s.palette = p;
        } else {
            eprintln!("settings: palette \"{p}\" unknown, use one of {}", PALETTE_NAMES.join(", "));
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
    if let Some(q) = f.split.and_then(|t| t.quiet_edge) {
        match q.as_str() {
            "dim" => s.split.quiet_edge = QuietEdge::Dim,
            "off" => s.split.quiet_edge = QuietEdge::Off,
            _ => eprintln!("settings: split quiet_edge \"{q}\" unknown, use dim or off"),
        }
    }
    if let Some(r) = f.ripple {
        let s = &mut s.ripple;
        in_range("ripple wave_seconds", r.wave_seconds, 0.3, 2.0, &mut s.wave_seconds);
        in_range("ripple tail", r.tail, 0.2, 3.0, &mut s.tail);
        in_range("ripple sparks", r.sparks, 0.0, 2.0, &mut s.sparks);
    }
}

fn in_range(name: &str, value: Option<f32>, min: f32, max: f32, into: &mut f32) {
    match value {
        Some(v) if (min..=max).contains(&v) => *into = v,
        Some(v) => eprintln!("settings: {name} {v} out of range {min} to {max}"),
        None => {}
    }
}

/// One value to record in the file: its table (`None` for the top level), its
/// key and its TOML text.
pub type Change = (Option<&'static str>, &'static str, String);

/// The file lines that turn `old` into `new`, one per value that differs.
pub fn changes(old: &Settings, new: &Settings) -> Vec<Change> {
    let text = |s: &str| toml::Value::String(s.to_string()).to_string();
    let quiet = |q: QuietEdge| text(if q == QuietEdge::Dim { "dim" } else { "off" });
    let layout = |l: Layout| text(if l == Layout::Strips { "strips" } else { "full" });
    let float = |v: f32| format!("{v:?}");
    let list = |names: &[String]| toml::Value::Array(names.iter().map(|n| toml::Value::String(n.clone())).collect()).to_string();
    let (o, n) = (old, new);
    let all: [(bool, Option<&'static str>, &'static str, String); 14] = [
        (o.enabled != n.enabled, None, "enabled", n.enabled.to_string()),
        (o.toggle_key != n.toggle_key, None, "toggle_key", text(&n.toggle_key)),
        (o.start_with_windows != n.start_with_windows, None, "start_with_windows", n.start_with_windows.to_string()),
        (o.theme != n.theme, None, "theme", text(&n.theme)),
        (o.palette != n.palette, None, "palette", text(&n.palette)),
        (o.album_colors != n.album_colors, None, "album_colors", n.album_colors.to_string()),
        (o.fps != n.fps, None, "fps", n.fps.to_string()),
        (o.layout != n.layout, None, "layout", layout(n.layout)),
        (o.monitors != n.monitors, None, "monitors", list(&n.monitors)),
        (o.pause_on_fullscreen != n.pause_on_fullscreen, None, "pause_on_fullscreen", n.pause_on_fullscreen.to_string()),
        (o.split.quiet_edge != n.split.quiet_edge, Some("split"), "quiet_edge", quiet(n.split.quiet_edge)),
        (o.ripple.wave_seconds != n.ripple.wave_seconds, Some("ripple"), "wave_seconds", float(n.ripple.wave_seconds)),
        (o.ripple.tail != n.ripple.tail, Some("ripple"), "tail", float(n.ripple.tail)),
        (o.ripple.sparks != n.ripple.sparks, Some("ripple"), "sparks", float(n.ripple.sparks)),
    ];
    all.into_iter().filter(|c| c.0).map(|(_, table, key, value)| (table, key, value)).collect()
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

    #[test]
    fn bad_values_keep_defaults_and_good_ones_apply() {
        let s = parse("palette = \"pink\"\nfps = 5\nlayout = \"full\"\n");
        assert_eq!(s, Settings { layout: Layout::Full, ..Settings::default() });
        let s = parse("palette = \"violet\"\nfps = 30\n");
        assert_eq!(s, Settings { palette: "violet".into(), fps: 30, ..Settings::default() });
        let s = parse("theme = \"none\"\n");
        assert_eq!(s, Settings::default());
        let s = parse("theme = \"split\"\n[split]\nquiet_edge = \"off\"\n");
        assert_eq!(s.theme, "split");
        assert_eq!(s.split.quiet_edge, QuietEdge::Off);
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
        assert_eq!(s.ripple, RippleSettings { wave_seconds: 1.5, tail: 1.0, sparks: 0.0 });
    }

    #[test]
    fn a_value_changes_only_its_own_line() {
        let with_enabled = |text: &str, on: bool| with_value(text, None, "enabled", &on.to_string());
        let text = "# mine\nenabled = true # note\ntheme = \"split\"\n\n[split]\nquiet_edge = \"off\"\n";
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
        let tables = "tail = 9\n[split]\nquiet_edge = \"dim\"\n\n[ripple]\ntail = 2.0\n";
        let tail = with_value(tables, Some("ripple"), "tail", "0.5");
        assert_eq!(tail, tables.replace("tail = 2.0", "tail = 0.5"));
        let sparks = with_value(tables, Some("ripple"), "sparks", "0.0");
        assert_eq!(sparks, tables.replace("[ripple]\n", "[ripple]\nsparks = 0.0\n"));
        let missing = with_value("fps = 30\n", Some("split"), "quiet_edge", "\"off\"");
        assert_eq!(missing, "fps = 30\n\n[split]\nquiet_edge = \"off\"\n");
        assert_eq!(parse(&missing).split.quiet_edge, QuietEdge::Off);
    }

    /// Every field the window can change reaches the file and reads back,
    /// judged by the parser rather than by the writer.
    #[test]
    fn every_change_reads_back() {
        let changed = Settings {
            enabled: false,
            toggle_key: "Win+Shift+F9".into(),
            start_with_windows: true,
            theme: "ripple".into(),
            palette: "violet".into(),
            album_colors: true,
            fps: 30,
            layout: Layout::Full,
            monitors: vec!["Right \"quoted\"".into(), PRIMARY.into()],
            pause_on_fullscreen: true,
            split: SplitSettings { quiet_edge: QuietEdge::Off },
            ripple: RippleSettings { wave_seconds: 1.25, tail: 0.35, sparks: 0.0 },
        };
        let lines = changes(&Settings::default(), &changed);
        assert_eq!(lines.len(), 14, "{lines:?}");
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
    }
}

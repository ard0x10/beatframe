#![cfg_attr(windows, windows_subsystem = "windows")]

mod album;
mod autostart;
mod focus;
mod fullscreen;
mod icon;
mod instance;
mod monitors;
mod overlay;
mod preview;
mod settings;
mod settings_window;
mod signal;
mod theme;
mod tray;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{MenuEvent, MenuId};
use tray_icon::TrayIconEvent;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use focus::Dimmer;
use fullscreen::Gate;
use monitors::Monitor;
use overlay::{Layout, Overlay, Uniforms};
use settings::{Overrides, Settings};
use settings_window::SettingsWindow;
use signal::{Shared, WAKE_LEVEL};
use theme::Theme;
use tray::{Action, Tray};

/// Base color for the rim and the kick, accent for the snare and hi-hat.
#[derive(Clone, Copy, PartialEq)]
struct Palette {
    base: [f32; 4],
    accent: [f32; 4],
}

const fn rgb(hex: u32) -> [f32; 4] {
    [((hex >> 16) & 0xff) as f32 / 255.0, ((hex >> 8) & 0xff) as f32 / 255.0, (hex & 0xff) as f32 / 255.0, 1.0]
}

const PALETTES: [(&str, Palette); 3] = [
    ("jade", Palette { base: rgb(0x10b8a0), accent: rgb(0x6c8cff) }),
    ("ember", Palette { base: rgb(0x0a0505), accent: rgb(0xe80a0a) }),
    ("violet", Palette { base: rgb(0x7a3cff), accent: rgb(0xff4fb0) }),
];

fn palette(name: &str) -> Palette {
    PALETTES.iter().find(|(n, _)| *n == name).map_or(PALETTES[0].1, |(_, p)| *p)
}

/// The palette the settings name, or the two colors the user picked.
fn colors_of(settings: &Settings) -> Palette {
    if settings.palette != settings::CUSTOM {
        return palette(&settings.palette);
    }
    let rgb = |c: [u8; 3]| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0];
    Palette { base: rgb(settings.custom_base), accent: rgb(settings.custom_accent) }
}

/// How long the light takes to move to new colors.
const FADE: Duration = Duration::from_millis(1500);

/// A move from one palette to another, eased over `FADE`.
struct Fade {
    from: Palette,
    to: Palette,
    start: Instant,
}

impl Fade {
    fn still(p: Palette) -> Self {
        Fade { from: p, to: p, start: Instant::now() - FADE }
    }

    fn running(&self, now: Instant) -> bool {
        now - self.start < FADE
    }

    fn at(&self, now: Instant) -> Palette {
        let t = (now - self.start).as_secs_f32() / FADE.as_secs_f32();
        if t >= 1.0 {
            return self.to;
        }
        let t = t * t * (3.0 - 2.0 * t);
        let mix = |a: [f32; 4], b: [f32; 4]| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        Palette { base: mix(self.from.base, self.to.base), accent: mix(self.from.accent, self.to.accent) }
    }
}

enum UserEvent {
    /// Sound came back while the overlay was idle.
    Wake,
    Menu(MenuId),
    SettingsChanged,
    /// A registered key combination was pressed.
    HotKey(u32),
    /// The cover of the music playing gave colors, or there is none to go by.
    Album(Option<album::Colors>),
    /// Another window came to the foreground.
    Foreground,
    /// Windows changed the monitors: one came or went, or its size, place or
    /// role changed.
    Displays,
    /// The app was started again while this copy runs.
    ShowSettings,
}

/// How long after a display change the monitors are read, so a burst of
/// changes while Windows settles is read once.
const DISPLAY_SETTLE: Duration = Duration::from_millis(300);

/// How often the light is put back above the taskbar and other always-on-top
/// windows, for the times one rises without the foreground changing.
const RAISE: Duration = Duration::from_secs(1);

/// A monitor the light is drawn on, with its own full screen check.
struct Lit {
    monitor: Monitor,
    gate: Gate,
}

struct Stats {
    since: Instant,
    frames: u32,
    draw_time: Duration,
}

struct App {
    settings_path: PathBuf,
    overrides: Overrides,
    settings: Settings,
    /// What the file holds as far as this app knows: read from it or written to it.
    on_disk: Settings,
    settings_window: Option<SettingsWindow>,
    proxy: EventLoopProxy<UserEvent>,
    hotkeys: Option<GlobalHotKeyManager>,
    toggle_key: Option<HotKey>,
    colors: Fade,
    album: Option<Palette>,
    watching_album: bool,
    frame: Duration,
    /// Switched on from the tray.
    enabled: bool,
    /// On at least one screen: enabled and not stepping aside everywhere for
    /// full screen windows.
    shown: bool,
    /// Connected now, as last read.
    monitors: Vec<Monitor>,
    lit: Vec<Lit>,
    /// When to read the monitors again after Windows said they changed.
    display_check: Option<Instant>,
    /// A window of the light changed scale and must be put back in place.
    resettle: bool,
    /// A slider in the settings window is held; the light's windows are not
    /// rebuilt for a new thickness until it is let go.
    holding: bool,
    next_check: Instant,
    next_raise: Instant,
    /// Focus mode: how much of the light shows, and when the keyboard and
    /// mouse are next checked.
    dimmer: Dimmer,
    next_focus: Instant,
    stop_at: Option<Instant>,
    shared: Arc<Shared>,
    overlay: Option<Overlay>,
    tray: Option<Tray>,
    theme: Box<dyn Theme>,
    started: Instant,
    last_tick: Instant,
    next_frame: Instant,
    animating: bool,
    stats: Stats,
}

impl App {
    fn apply(&mut self, settings: Settings, event_loop: &ActiveEventLoop) {
        self.frame = Duration::from_nanos(1_000_000_000 / settings.fps as u64);
        let mut relayout = settings.layout != self.settings.layout;
        if settings.theme != self.settings.theme {
            if let Some(t) = theme::create(&settings.theme, &settings) {
                relayout |= t.reach() != self.theme.reach();
                self.theme = t;
                if !relayout && let Some(o) = self.overlay.as_mut() {
                    o.set_shader(&theme::shader(self.theme.as_ref()));
                }
            }
        } else {
            // A slider being dragged lands here every frame; the shader stays.
            self.theme.configure(&settings);
        }
        // A new thickness needs deeper or shallower strips, once the slider is let go.
        relayout |= !self.holding && self.overlay.as_ref().is_some_and(|o| o.reach() != self.theme.reach());
        if !settings.pause_on_fullscreen {
            self.lit.iter_mut().for_each(|l| l.gate = Gate::default());
        }
        self.next_check = Instant::now();
        let autostart_changed = settings.start_with_windows != self.settings.start_with_windows;
        // While the window waits for a new combination the old one stays let go.
        let key_changed = settings.toggle_key != self.settings.toggle_key
            && !self.settings_window.as_ref().is_some_and(|w| w.recording_key());
        self.settings = settings;
        if key_changed {
            self.register_toggle_key();
        }
        if autostart_changed && self.stop_at.is_none() {
            autostart::set(self.settings.start_with_windows);
        }
        if self.settings.enabled != self.enabled {
            self.set_enabled(self.settings.enabled, event_loop);
        }
        self.show_in_tray();
        self.next_focus = Instant::now();
        self.aim_focus();
        self.watch_album();
        self.retarget(Instant::now());
        if self.relight() || relayout {
            self.rebuild(event_loop);
        } else if self.shown {
            self.start_animating();
        }
        self.refresh(event_loop);
        if let Some(w) = &self.settings_window {
            w.request_redraw();
        }
    }

    /// Picks the monitors to draw on from the settings and the monitors
    /// connected. Returns true when they changed; each starts with a fresh
    /// full screen check.
    fn relight(&mut self) -> bool {
        let lit: Vec<&Monitor> = monitors::lit(&self.settings.monitors, &self.monitors);
        if lit.len() == self.lit.len() && lit.iter().zip(&self.lit).all(|(m, l)| **m == l.monitor) {
            return false;
        }
        let names: Vec<&str> = lit.iter().map(|m| m.name.as_str()).collect();
        eprintln!("monitors: light on {names:?}");
        self.lit = lit.into_iter().map(|m| Lit { monitor: m.clone(), gate: Gate::default() }).collect();
        true
    }

    /// Closes the light's windows and opens them again where they now belong,
    /// checking for full screen windows first so a game never gets a flash.
    fn rebuild(&mut self, event_loop: &ActiveEventLoop) {
        self.overlay = None;
        self.lit.iter_mut().for_each(|l| l.gate = Gate::default());
        if self.watching_fullscreen() {
            self.check_fullscreen(Instant::now(), event_loop);
        } else {
            self.refresh(event_loop);
        }
    }

    /// Reads the monitors again after Windows changed them.
    fn reread_monitors(&mut self, event_loop: &ActiveEventLoop) {
        let now = monitors::connected();
        if now != self.monitors {
            eprintln!("monitors: connected {:?}", monitors::describe(&now));
            self.monitors = now;
            if let Some(w) = self.settings_window.as_mut() {
                w.set_monitors(&self.monitors);
            }
            self.show_in_tray();
        }
        if self.relight() {
            self.rebuild(event_loop);
        } else if let Some(o) = &self.overlay {
            // Same monitors, but Windows may have moved the windows while it
            // rearranged them.
            o.settle();
        }
    }

    fn open_overlay(&mut self, event_loop: &ActiveEventLoop) {
        let rects: Vec<_> = self.lit.iter().map(|l| l.monitor.rect).collect();
        self.overlay = Some(Overlay::new(
            event_loop,
            &rects,
            self.settings.layout,
            self.theme.reach(),
            &theme::shader(self.theme.as_ref()),
        ));
    }

    fn set_enabled(&mut self, on: bool, event_loop: &ActiveEventLoop) {
        self.enabled = on;
        self.show_in_tray();
        // Checked before showing, so switching on over a game does not flash.
        self.lit.iter_mut().for_each(|l| l.gate = Gate::default());
        if self.watching_fullscreen() {
            self.check_fullscreen(Instant::now(), event_loop);
        } else {
            self.refresh(event_loop);
        }
    }

    /// Switches the light and records it, from the tray or the key combination.
    fn toggle(&mut self, event_loop: &ActiveEventLoop) {
        let on = !self.enabled;
        self.set_enabled(on, event_loop);
        // Recorded in the file so the next start remembers it; the reload that
        // follows finds nothing new.
        self.settings.enabled = on;
        self.on_disk.enabled = on;
        settings::write(&self.settings_path, &[(None, "enabled", on.to_string())]);
        if let Some(w) = &self.settings_window {
            w.request_redraw();
        }
    }

    /// A change made from the tray: recorded in the file, then applied.
    fn change(&mut self, edit: impl FnOnce(&mut Settings), event_loop: &ActiveEventLoop) {
        let mut s = self.settings.clone();
        edit(&mut s);
        let changes = settings::changes(&self.on_disk, &s);
        eprintln!("settings: from the tray {changes:?}");
        settings::write(&self.settings_path, &changes);
        self.on_disk = s.clone();
        self.apply(s, event_loop);
        // A click flips its own check mark even when nothing changed.
        self.show_in_tray();
    }

    fn show_in_tray(&mut self) {
        if let Some(t) = self.tray.as_mut() {
            t.show(self.enabled, &self.settings.monitors, &self.monitors);
        }
    }

    /// Points the light at full or at the focus level from how long the
    /// keyboard and mouse have been still. The settings window keeps it at
    /// full, since there the user is looking at the light itself.
    fn aim_focus(&mut self) {
        let tuning = self.settings_window.as_ref().is_some_and(|w| w.focused());
        let target = if self.settings.focus_mode && !tuning {
            let after = Duration::from_secs_f32(self.settings.focus_after);
            focus::target(focus::idle(), after, self.settings.focus_level)
        } else {
            1.0
        };
        if self.dimmer.aim(target) && self.shown {
            self.start_animating();
        }
    }

    fn watching_focus(&self) -> bool {
        self.shown && self.settings.focus_mode
    }

    fn open_settings(&mut self, event_loop: &ActiveEventLoop) {
        match &self.settings_window {
            Some(w) => w.raise(),
            None => {
                self.settings_window =
                    SettingsWindow::open(event_loop, &self.settings, self.colors.at(Instant::now()), &self.monitors)
            }
        }
    }

    /// Draws the settings window and takes up whatever it changed: applied at
    /// once, written to the file once no slider is held.
    fn settings_frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(w) = self.settings_window.as_mut() else { return };
        let was_recording = w.recording_key();
        let outcome = w.redraw(&self.settings, self.colors.at(Instant::now()));
        let recording = w.recording_key();
        let was_holding = std::mem::replace(&mut self.holding, !outcome.settled);
        if outcome.settings != self.settings || (was_holding && outcome.settled) {
            self.apply(outcome.settings, event_loop);
        }
        if outcome.settled {
            let changes = settings::changes(&self.on_disk, &self.settings);
            if !changes.is_empty() {
                eprintln!("settings: from the window {changes:?}");
                settings::write(&self.settings_path, &changes);
                self.on_disk = self.settings.clone();
            }
        }
        if recording != was_recording {
            // Windows hands a registered combination to its owner, not to the
            // focused window, so the key is let go while a new one is pressed.
            if recording {
                self.release_toggle_key();
            } else {
                self.register_toggle_key();
            }
        }
    }

    fn close_settings(&mut self) {
        if self.settings_window.take().is_some_and(|w| w.recording_key()) {
            self.register_toggle_key();
        }
    }

    fn release_toggle_key(&mut self) {
        if let (Some(manager), Some(old)) = (&self.hotkeys, self.toggle_key.take()) {
            let _ = manager.unregister(old);
        }
    }

    fn register_toggle_key(&mut self) {
        self.release_toggle_key();
        let Some(manager) = &self.hotkeys else { return };
        let text = &self.settings.toggle_key;
        let Some(key) = settings::parse_key(text) else { return };
        match manager.register(key) {
            Ok(()) => self.toggle_key = Some(key),
            Err(e) => eprintln!("hotkey: {text} not available, another app may hold it: {e}"),
        }
    }

    /// Starts following the media session the first time album colors are on.
    fn watch_album(&mut self) {
        if self.settings.album_colors && !self.watching_album {
            self.watching_album = true;
            let proxy = self.proxy.clone();
            album::spawn(move |colors| {
                let _ = proxy.send_event(UserEvent::Album(colors));
            });
        }
    }

    /// Fades to the album's colors when they are on and known, else to the palette.
    fn retarget(&mut self, now: Instant) {
        let target = match self.album {
            Some(p) if self.settings.album_colors => p,
            _ => colors_of(&self.settings),
        };
        if target != self.colors.to {
            self.colors = Fade { from: self.colors.at(now), to: target, start: now };
            if self.shown {
                self.start_animating();
            }
        }
    }

    fn watching_fullscreen(&self) -> bool {
        self.enabled && self.settings.pause_on_fullscreen
    }

    /// Checks every lit monitor against the foreground window: only the
    /// monitor it covers steps aside.
    fn check_fullscreen(&mut self, now: Instant, event_loop: &ActiveEventLoop) {
        let rects: Vec<_> = self.lit.iter().map(|l| l.monitor.rect).collect();
        let covered = fullscreen::covered(fullscreen::foreground_frame(), &rects);
        for (l, covered) in self.lit.iter_mut().zip(covered) {
            let was_hidden = l.gate.hidden();
            l.gate.update(covered, now);
            if l.gate.hidden() != was_hidden {
                let what = if was_hidden { "uncovered, light back" } else { "covered, light hidden" };
                eprintln!("fullscreen: {} {what}", l.monitor.name);
            }
        }
        self.next_check = self.lit.iter().map(|l| l.gate.next_check(now)).min().unwrap_or(now + fullscreen::POLL);
        self.refresh(event_loop);
    }

    /// Which lit monitors show the light: all of them while it is switched
    /// on, less those a full screen window covers.
    fn visible(&self) -> Vec<bool> {
        self.lit.iter().map(|l| self.enabled && !l.gate.hidden()).collect()
    }

    /// Shows or hides the light on each monitor to match the tray switch and
    /// the full screen checks. Sound is analyzed while any monitor shows it.
    fn refresh(&mut self, event_loop: &ActiveEventLoop) {
        let visible = self.visible();
        let on = visible.contains(&true);
        if on && self.overlay.is_none() {
            self.open_overlay(event_loop);
        }
        let appeared = self.overlay.as_mut().is_some_and(|o| o.set_visible(&visible));
        if on != self.shown {
            self.shown = on;
            self.shared.paused.store(!on, Ordering::Release);
            if !on {
                self.animating = false;
                self.theme.reset();
            }
        }
        if appeared {
            // The next frame draws it, then shows it.
            self.start_animating();
        }
    }

    fn start_animating(&mut self) {
        if !self.animating {
            let now = Instant::now();
            self.animating = true;
            self.last_tick = now;
            self.next_frame = now;
        }
    }

    fn frame(&mut self, now: Instant) {
        let Some(overlay) = self.overlay.as_mut() else { return };
        let dt = (now - self.last_tick).as_secs_f32().min(0.1);
        self.last_tick = now;
        let snapshot = self.shared.snapshot();
        self.theme.update(&snapshot, dt);
        self.dimmer.update(dt);

        let colors = self.colors.at(now);
        let settled = self.theme.settled()
            && snapshot.level <= WAKE_LEVEL
            && !self.colors.running(now)
            && self.dimmer.settled();
        if settled {
            // The last frame drawn is the resting one, which stays on screen.
            self.theme.reset();
        }
        let started = Instant::now();
        overlay.draw(Uniforms {
            // Each window fills in its own place and its monitor's size.
            origin: [0.0; 2],
            screen: [0.0; 2],
            base: colors.base,
            accent: colors.accent,
            time: (now - self.started).as_secs_f32() % 1000.0,
            thickness: 1.0,
            brightness: 1.0,
            resting: 1.0,
            params: self.theme.params(),
            shown: self.dimmer.shown,
            _pad: [0.0; 3],
        }
        .with_look(self.theme.look()));
        self.stats.draw_time += started.elapsed();
        self.stats.frames += 1;

        if settled {
            self.animating = false;
            self.shared.idle.store(true, Ordering::Release);
            // A hit that landed between the snapshot and going idle would otherwise be lost.
            if self.shared.snapshot().hits != snapshot.hits && self.shared.idle.swap(false, Ordering::AcqRel) {
                self.animating = true;
            }
        }
    }

    fn report(&mut self, now: Instant) {
        let span = now - self.stats.since;
        if span < Duration::from_secs(5) {
            return;
        }
        let frames = self.stats.frames.max(1);
        eprintln!(
            "overlay: {} {:.1} fps, draw {:.2} ms/frame, {}",
            self.theme.name(),
            self.stats.frames as f64 / span.as_secs_f64(),
            self.stats.draw_time.as_secs_f64() * 1000.0 / frames as f64,
            if !self.enabled {
                "off"
            } else if !self.shown {
                "hidden for full screen"
            } else if self.animating {
                "animating"
            } else {
                "idle"
            }
        );
        self.stats = Stats { since: now, frames: 0, draw_time: Duration::ZERO };
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.tray.is_none() {
            self.tray = Some(Tray::new(self.enabled));
            self.show_in_tray();
        }
        self.watch_album();
        if self.overlay.is_none() {
            self.relight();
            self.rebuild(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Wake => {
                if self.shown {
                    self.start_animating();
                }
            }
            UserEvent::HotKey(id) => {
                if self.toggle_key.is_some_and(|k| k.id() == id) {
                    self.toggle(event_loop);
                }
            }
            UserEvent::Album(colors) => {
                self.album = colors.map(|(base, accent)| Palette {
                    base: [base[0], base[1], base[2], 1.0],
                    accent: [accent[0], accent[1], accent[2], 1.0],
                });
                self.retarget(Instant::now());
            }
            UserEvent::Foreground => {
                if self.watching_fullscreen() {
                    self.check_fullscreen(Instant::now(), event_loop);
                }
                // Clicking the taskbar brings it to the foreground and over the light.
                if let Some(o) = &self.overlay {
                    o.raise();
                }
            }
            UserEvent::Displays => {
                self.display_check.get_or_insert(Instant::now() + DISPLAY_SETTLE);
            }
            UserEvent::ShowSettings => self.open_settings(event_loop),
            UserEvent::SettingsChanged => {
                if let Some(s) = settings::load(&self.settings_path, &self.overrides) {
                    self.on_disk = s.clone();
                    if s != self.settings {
                        eprintln!("settings: {s:?}");
                        self.apply(s, event_loop);
                    }
                }
            }
            UserEvent::Menu(id) => match self.tray.as_ref().and_then(|t| t.action(&id)) {
                Some(Action::Toggle) => self.toggle(event_loop),
                Some(Action::Monitor(m, on)) => {
                    let connected = self.monitors.clone();
                    self.change(|s| s.monitors = monitors::choose(&s.monitors, &connected, &m, on), event_loop)
                }
                Some(Action::OpenSettings) => self.open_settings(event_loop),
                Some(Action::Quit) => event_loop.exit(),
                None => {}
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(w) = self.settings_window.as_mut().filter(|w| w.id() == id) else {
            // The overlay's windows cannot be closed from the screen. Windows
            // rescales one whose monitor changes scale; it goes back in place.
            if matches!(event, WindowEvent::ScaleFactorChanged { .. })
                && self.overlay.as_ref().is_some_and(|o| o.owns(id))
            {
                self.resettle = true;
            }
            return;
        };
        if let WindowEvent::RedrawRequested = event {
            self.settings_frame(event_loop);
        } else if w.event(&event) {
            self.close_settings();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.stop_at.is_some_and(|t| now >= t) {
            event_loop.exit();
            return;
        }
        self.report(now);
        if self.display_check.is_some_and(|t| now >= t) {
            self.display_check = None;
            self.reread_monitors(event_loop);
        }
        if std::mem::take(&mut self.resettle)
            && let Some(o) = &self.overlay
        {
            o.settle();
        }
        let watching = self.watching_fullscreen();
        if watching && now >= self.next_check {
            self.check_fullscreen(now, event_loop);
        }
        let focusing = self.watching_focus();
        if focusing && now >= self.next_focus {
            self.next_focus = now + focus::POLL;
            self.aim_focus();
        }
        if self.shown && now >= self.next_raise {
            self.next_raise = now + RAISE;
            if let Some(o) = &self.overlay {
                o.raise();
            }
        }
        let check = watching.then_some(self.next_check);
        let raise = self.shown.then_some(self.next_raise);
        let window = self.settings_window.as_mut().and_then(|w| w.poll(now));
        let displays = self.display_check;
        let focus = focusing.then_some(self.next_focus);
        let sooner = |wake: Instant| [check, raise, window, displays, focus].into_iter().flatten().fold(wake, Instant::min);
        if !self.animating {
            let wake = self.stats.since + Duration::from_secs(5);
            let wake = self.stop_at.map_or(wake, |t| t.min(wake));
            event_loop.set_control_flow(ControlFlow::WaitUntil(sooner(wake)));
            return;
        }
        if now >= self.next_frame {
            self.frame(now);
            self.next_frame = (self.next_frame + self.frame).max(now);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(sooner(self.next_frame)));
    }
}

fn usage() -> ! {
    eprintln!("usage: beatframe [--strips | --full] [--theme NAME] [--palette NAME] [--fps N] [--seconds N]");
    std::process::exit(2);
}

fn main() {
    let mut overrides = Overrides::default();
    let mut stop_at = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--full" => overrides.layout = Some(Layout::Full),
            "--strips" => overrides.layout = Some(Layout::Strips),
            "--theme" => {
                let name = args.next().unwrap_or_default();
                if !theme::NAMES.contains(&name.as_str()) {
                    eprintln!("--theme takes one of {}", theme::NAMES.join(", "));
                    usage();
                }
                overrides.theme = Some(name);
            }
            "--palette" => {
                let name = args.next().unwrap_or_default();
                if !settings::PALETTE_NAMES.contains(&name.as_str()) {
                    eprintln!("--palette takes one of {}", settings::PALETTE_NAMES.join(", "));
                    usage();
                }
                overrides.palette = Some(name);
            }
            "--fps" => match args.next().and_then(|v| v.parse().ok()).filter(|&f| f > 0) {
                Some(f) => overrides.fps = Some(f),
                None => usage(),
            },
            "--seconds" => match args.next().and_then(|v| v.parse::<u64>().ok()) {
                Some(s) => stop_at = Some(Instant::now() + Duration::from_secs(s)),
                None => usage(),
            },
            _ => usage(),
        }
    }

    if !instance::first() {
        eprintln!("already running, asking it for the settings window");
        instance::ask_for_settings();
        return;
    }

    let settings_path = settings::path();
    settings::ensure_file(&settings_path);
    let settings = settings::load(&settings_path, &overrides).unwrap_or_else(|| settings::load_defaults(&overrides));
    eprintln!("settings: {} {settings:?}", settings_path.display());

    let event_loop = EventLoop::<UserEvent>::with_user_event().build().expect("event loop");
    let shared = Arc::new(Shared::default());
    shared.paused.store(!settings.enabled, Ordering::Release);
    let proxy = event_loop.create_proxy();
    signal::spawn(shared.clone(), move || {
        let _ = proxy.send_event(UserEvent::Wake);
    });
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = proxy.send_event(UserEvent::Menu(e.id));
    }));
    let proxy = event_loop.create_proxy();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state == HotKeyState::Pressed {
            let _ = proxy.send_event(UserEvent::HotKey(e.id));
        }
    }));
    // Its hidden window lives on this thread, whose event loop delivers the keys.
    let hotkeys = GlobalHotKeyManager::new().map_err(|e| eprintln!("hotkey: unavailable: {e}")).ok();
    // Without a handler, clicks and hovers on the icon pile up in an unread queue.
    TrayIconEvent::set_event_handler(Some(|_| {}));
    let proxy = event_loop.create_proxy();
    let _watcher = settings::watch(&settings_path, move || {
        let _ = proxy.send_event(UserEvent::SettingsChanged);
    });
    let proxy = event_loop.create_proxy();
    let _foreground = fullscreen::watch(move || {
        let _ = proxy.send_event(UserEvent::Foreground);
    });
    let proxy = event_loop.create_proxy();
    instance::listen(move || {
        let _ = proxy.send_event(UserEvent::ShowSettings);
    });
    let proxy = event_loop.create_proxy();
    let _displays = monitors::watch(move || {
        let _ = proxy.send_event(UserEvent::Displays);
    });
    let connected = monitors::connected();
    eprintln!("monitors: connected {:?}", monitors::describe(&connected));

    let theme = theme::create(&settings.theme, &settings).expect("settings only hold known themes");
    let now = Instant::now();
    let mut app = App {
        settings_path,
        overrides,
        on_disk: settings.clone(),
        settings_window: None,
        proxy: event_loop.create_proxy(),
        hotkeys,
        toggle_key: None,
        colors: Fade::still(colors_of(&settings)),
        album: None,
        watching_album: false,
        frame: Duration::from_nanos(1_000_000_000 / settings.fps as u64),
        enabled: settings.enabled,
        // Set by the first refresh, once the monitors are known.
        shown: false,
        settings,
        monitors: connected,
        lit: Vec::new(),
        display_check: None,
        resettle: false,
        holding: false,
        next_check: now,
        next_raise: now,
        dimmer: Dimmer::default(),
        next_focus: now,
        stop_at,
        shared,
        overlay: None,
        tray: None,
        theme,
        started: now,
        last_tick: now,
        next_frame: now,
        animating: false,
        stats: Stats { since: now, frames: 0, draw_time: Duration::ZERO },
    };
    app.register_toggle_key();
    // A timed run is a measurement and leaves the sign-in entry alone.
    if app.stop_at.is_none() {
        autostart::set(app.settings.start_with_windows);
    }
    event_loop.run_app(&mut app).expect("running the event loop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fade_starts_on_the_old_colors_and_ends_on_the_new() {
        let jade = palette("jade");
        let violet = palette("violet");
        let start = Instant::now();
        let fade = Fade { from: jade, to: violet, start };
        assert!(fade.at(start) == jade);
        assert!(fade.running(start + FADE / 2));
        let middle = fade.at(start + FADE / 2).base;
        for i in 0..3 {
            let (a, b) = (jade.base[i].min(violet.base[i]), jade.base[i].max(violet.base[i]));
            assert!(middle[i] > a && middle[i] < b, "channel {i}: {} not between {a} and {b}", middle[i]);
        }
        assert!(!fade.running(start + FADE));
        assert!(fade.at(start + FADE) == violet);
        assert!(Fade::still(jade).at(start) == jade && !Fade::still(jade).running(Instant::now()));
    }
}

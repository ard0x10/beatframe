//! The settings window: an egui page on its own small GPU device, made when the
//! tray asks for it and dropped, device and all, when it closes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Button, FontData, FontDefinitions, FontFamily, RichText, Slider, TextStyle, Ui, ViewportId};
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Icon, Window, WindowId};

use crate::icon;
use crate::monitors::{self, Monitor};
use crate::overlay::Uniforms;
use crate::preview::{self, Preview};
use crate::settings::{self, Settings};
use crate::tray;
use crate::Palette;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Look,
    Behavior,
}

/// The key field while it waits for a combination.
#[derive(Default)]
struct KeyField {
    recording: bool,
    note: Option<&'static str>,
}

/// What one pass over the page decided.
pub struct Outcome {
    /// The settings as the page left them; equal to the input when nothing changed.
    pub settings: Settings,
    /// False while a slider is held, so the file is written once on release.
    pub settled: bool,
}

pub struct SettingsWindow {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    state: egui_winit::State,
    ctx: egui::Context,
    page: Page,
    key: KeyField,
    preview: Preview,
    /// Width over height of the screen the light is on, for the preview's shape.
    aspect: f32,
    /// The preview plays only while the window has the keyboard focus.
    focused: bool,
    /// Connected now, for the monitor list.
    monitors: Vec<Monitor>,
    repaint_at: Option<Instant>,
}

/// What the preview draws with on one pass.
struct View<'a> {
    preview: &'a mut Preview,
    colors: Palette,
    aspect: f32,
    playing: bool,
    /// Set by the preview when it wants its next frame, this long after this one began.
    next_frame: Option<Duration>,
}

impl SettingsWindow {
    /// Opens the window, or returns `None` when Windows has no font to draw it with.
    pub fn open(event_loop: &ActiveEventLoop, settings: &Settings, colors: Palette, connected: &[Monitor]) -> Option<Self> {
        let fonts = system_fonts()?;
        let icon = Icon::from_rgba(icon::rgba(tray::ICON_SIZE), tray::ICON_SIZE, tray::ICON_SIZE).ok();
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("BeatFrame")
                        .with_inner_size(LogicalSize::new(580.0, 620.0))
                        .with_min_inner_size(LogicalSize::new(460.0, 360.0))
                        .with_window_icon(icon)
                        // Shown after the first frame, so it never flashes white.
                        .with_visible(false),
                )
                .map_err(|e| eprintln!("settings window: {e}"))
                .ok()?,
        );

        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::DX12;
        let instance = wgpu::Instance::new(desc);
        let surface = instance.create_surface(window.clone()).map_err(|e| eprintln!("settings window: {e}")).ok()?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| eprintln!("settings window: {e}"))
        .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("settings"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .map_err(|e| eprintln!("settings window: {e}"))
        .ok()?;
        let caps = surface.get_capabilities(&adapter);
        // egui blends in gamma space and expects a plain, non-sRGB target.
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface is supported by the adapter");
        config.format = format;
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        let mut renderer = egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
        renderer.callback_resources.insert(preview::Gpu::new(&device, format));
        let aspect = event_loop
            .primary_monitor()
            .or_else(|| window.current_monitor())
            .map(|m| m.size())
            .filter(|s| s.width > 0 && s.height > 0)
            .map_or(16.0 / 10.0, |s| s.width as f32 / s.height as f32);

        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        ctx.all_styles_mut(|style| {
            use egui::FontId;
            style.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
            style.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
            style.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
            style.text_styles.insert(TextStyle::Heading, FontId::proportional(17.0));
            style.spacing.item_spacing = egui::vec2(8.0, 7.0);
            style.spacing.slider_width = 180.0;
        });
        let state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );

        let mut this = SettingsWindow {
            window,
            surface,
            config,
            device,
            queue,
            renderer,
            state,
            ctx,
            page: Page::Look,
            key: KeyField::default(),
            preview: Preview::new(settings),
            aspect,
            focused: true,
            monitors: connected.to_vec(),
            repaint_at: None,
        };
        // Drawn once while hidden so the first thing on screen is the page.
        let _ = this.redraw(settings, colors);
        this.window.set_visible(true);
        this.window.focus_window();
        Some(this)
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    /// Brings the open window forward when the tray asks for it again.
    pub fn raise(&self) {
        self.window.set_minimized(false);
        self.window.focus_window();
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Takes the monitors connected after Windows changed them.
    pub fn set_monitors(&mut self, connected: &[Monitor]) {
        self.monitors = connected.to_vec();
        self.window.request_redraw();
    }

    /// True while the key field waits for a combination, when the current key
    /// must be let go so the window can see it pressed.
    pub fn recording_key(&self) -> bool {
        self.key.recording
    }

    /// Passes a window event to egui. Returns true for the close button.
    pub fn event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CloseRequested => return true,
            WindowEvent::Focused(focused) => {
                self.focused = *focused;
                self.window.request_redraw();
            }
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                self.config.width = size.width;
                self.config.height = size.height;
                self.surface.configure(&self.device, &self.config);
            }
            _ => {}
        }
        if self.state.on_window_event(&self.window, event).repaint {
            self.window.request_redraw();
        }
        false
    }

    /// Asks for a frame when egui's own timer (a hover fade, a blinking
    /// cursor) runs out, and returns when it next needs waking.
    pub fn poll(&mut self, now: Instant) -> Option<Instant> {
        if self.repaint_at.is_some_and(|t| now >= t) {
            self.repaint_at = None;
            self.window.request_redraw();
        }
        self.repaint_at
    }

    /// Runs the page over `settings` and draws it, the preview in `colors`.
    pub fn redraw(&mut self, settings: &Settings, colors: Palette) -> Outcome {
        let started = Instant::now();
        let input = self.state.take_egui_input(&self.window);
        let mut draft = settings.clone();
        let (page, key, connected) = (&mut self.page, &mut self.key, self.monitors.as_slice());
        let mut view =
            View { preview: &mut self.preview, colors, aspect: self.aspect, playing: self.focused, next_frame: None };
        let output = self.ctx.run_ui(input, |ui| show(ui, page, key, connected, &mut view, &mut draft));
        self.state.handle_platform_output(&self.window, output.platform_output);

        // The preview keeps its own time: egui takes a frame's length off every
        // delay it is asked for, which at the screen's own rate leaves nothing
        // and would draw the whole window as fast as the screen refreshes.
        let delay = output.viewport_output.get(&ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        if delay.is_zero() {
            self.window.request_redraw();
        }
        let egui_at = Instant::now().checked_add(delay).filter(|_| !delay.is_zero());
        let preview_at = view.next_frame.map(|d| started + d);
        if preview_at.is_none() {
            // Another page, or the window is behind another app: the beat
            // waits here instead of jumping ahead when the preview returns.
            self.preview.pause();
        }
        self.repaint_at = [egui_at, preview_at].into_iter().flatten().min();

        let jobs = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: output.pixels_per_point,
        };
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                None
            }
            _ => None,
        };
        if let Some(frame) = frame {
            let view = frame.texture.create_view(&Default::default());
            let mut encoder = self.device.create_command_encoder(&Default::default());
            let mut commands = self.renderer.update_buffers(&self.device, &self.queue, &mut encoder, &jobs, &screen);
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("settings"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                });
                self.renderer.render(&mut pass.forget_lifetime(), &jobs, &screen);
            }
            commands.push(encoder.finish());
            self.queue.submit(commands);
            self.window.pre_present_notify();
            self.queue.present(frame);
        }
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }

        Outcome { settings: draft, settled: self.ctx.dragged_id().is_none() }
    }
}

/// Segoe UI from the Windows fonts folder, read at run time so no font ships
/// inside the app. Segoe UI Symbol, when present, covers what Segoe UI lacks.
fn system_fonts() -> Option<FontDefinitions> {
    let dir = std::env::var_os("WINDIR").map_or_else(|| "C:\\Windows".into(), std::path::PathBuf::from).join("Fonts");
    let read = |name: &str| std::fs::read(dir.join(name)).ok();
    let Some(main) = read("segoeui.ttf") else {
        eprintln!("settings window: no Segoe UI in {}", dir.display());
        return None;
    };
    let mut fonts = FontDefinitions::empty();
    let mut names = Vec::new();
    for (name, data) in [("segoe-ui", Some(main)), ("segoe-ui-symbol", read("seguisym.ttf"))] {
        if let Some(data) = data {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(data)));
            names.push(name.to_string());
        }
    }
    fonts.families.insert(FontFamily::Proportional, names.clone());
    fonts.families.insert(FontFamily::Monospace, names);
    Some(fonts)
}

fn show(ui: &mut Ui, page: &mut Page, key: &mut KeyField, connected: &[Monitor], view: &mut View, s: &mut Settings) {
    egui::Panel::left("pages").resizable(false).exact_size(140.0).show(ui, |ui| {
        ui.add_space(10.0);
        for (p, name) in [(Page::Look, "Look"), (Page::Behavior, "Behavior")] {
            let selected = *page == p;
            let text = if selected { RichText::new(name).strong() } else { RichText::new(name) };
            let button = Button::selectable(selected, text).min_size(egui::vec2(ui.available_width(), 32.0));
            if ui.add(button).clicked() && !selected {
                *page = p;
                key.recording = false;
            }
        }
    });
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(4.0);
            match page {
                Page::Look => look(ui, view, s),
                Page::Behavior => behavior(ui, key, connected, s),
            }
        });
    });
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.heading(text);
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().weak());
}

const THEMES: [(&str, &str, &str); 5] = [
    ("layered", "Layered", "Every drum lights the whole edge, in layers."),
    ("split", "Split", "Kick lights the bottom, snare the sides, hi-hat the top."),
    ("ripple", "Ripple", "Each kick sends a wave up from the bottom middle."),
    ("aurora", "Aurora", "A slow curtain of light flows along the frame. The drums only nudge it."),
    ("band", "Band", "A thick band blending the two colors, which the music turns round the frame."),
];

fn look(ui: &mut Ui, view: &mut View, s: &mut Settings) {
    screen(ui, view, s);
    heading(ui, "Theme");
    ui.horizontal_wrapped(|ui| {
        for (name, label, _) in THEMES {
            ui.radio_value(&mut s.theme, name.to_string(), label);
        }
    });
    if let Some((_, _, about)) = THEMES.iter().find(|(n, ..)| *n == s.theme) {
        hint(ui, about);
    }
    theme_settings(ui, s);

    heading(ui, "Colors");
    ui.horizontal(|ui| {
        for name in settings::PALETTE_NAMES {
            ui.radio_value(&mut s.palette, name.to_string(), capitalized(name));
            swatch(ui, name);
            ui.add_space(6.0);
        }
    });
    ui.horizontal(|ui| {
        ui.radio_value(&mut s.palette, settings::CUSTOM.to_string(), "Custom");
        let base = egui::color_picker::color_edit_button_srgb(ui, &mut s.custom_base).changed();
        let accent = egui::color_picker::color_edit_button_srgb(ui, &mut s.custom_accent).changed();
        // Picking a color means using it.
        if base || accent {
            s.palette = settings::CUSTOM.to_string();
        }
    });
    hint(ui, "Custom: the first color lights the rim and the kick, the second the snare and the hi-hat.");
    ui.checkbox(&mut s.album_colors, "Colors from the album cover");
    hint(ui, "Follows the cover of the music playing. Other sound keeps the palette.");
}

/// The chosen theme's own values: what every theme has, then what only it has.
fn theme_settings(ui: &mut Ui, s: &mut Settings) {
    let theme = s.theme.clone();
    let label = THEMES.iter().find(|(n, ..)| *n == theme).map_or("Theme", |(_, l, _)| *l);
    let mut defaults = s.clone();
    defaults.reset_theme(&theme);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading(format!("{label} settings"));
        if ui.add_enabled(defaults != *s, Button::new("Reset")).clicked() {
            s.reset_theme(&theme);
        }
    });

    let slider = |ui: &mut Ui, name: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>| {
        ui.label(name);
        ui.add(Slider::new(value, range).step_by(0.05));
        ui.end_row();
    };
    egui::Grid::new("theme settings").num_columns(2).spacing(egui::vec2(12.0, 7.0)).show(ui, |ui| {
        let l = s.look_mut(&theme);
        slider(ui, "Thickness", &mut l.thickness, 0.5..=2.0);
        slider(ui, "Brightness", &mut l.brightness, 0.2..=2.0);
        for (drum, name) in l.drums.iter_mut().zip(["Kick", "Snare", "Hi-hat"]) {
            ui.checkbox(&mut drum.on, name);
            ui.add_enabled(drum.on, Slider::new(&mut drum.strength, 0.0..=2.0).step_by(0.05));
            ui.end_row();
        }
        slider(ui, "Fade", &mut l.fade, 0.3..=3.0);
        slider(ui, "Resting glow", &mut l.resting, 0.0..=2.0);

        match theme.as_str() {
            "layered" => slider(ui, "Shimmer", &mut s.layered.shimmer, 0.0..=2.0),
            "ripple" => {
                let r = &mut s.ripple;
                ui.label("Wave time");
                ui.add(Slider::new(&mut r.wave_seconds, 0.3..=2.0).step_by(0.05).suffix(" s"));
                ui.end_row();
                slider(ui, "Tail", &mut r.tail, 0.2..=3.0);
                slider(ui, "Sparks", &mut r.sparks, 0.0..=2.0);
            }
            "aurora" => {
                slider(ui, "Flow", &mut s.aurora.flow, 0.2..=3.0);
                slider(ui, "Folds", &mut s.aurora.folds, 0.5..=2.0);
            }
            "band" => {
                slider(ui, "Spin", &mut s.band.spin, 0.0..=3.0);
                slider(ui, "Waves", &mut s.band.waves, 0.0..=2.0);
                slider(ui, "Corners", &mut s.band.corners, 0.0..=1.0);
            }
            _ => {}
        }
    });
    hint(ui, "Thickness takes effect when you let go of the slider. A shorter fade is sharper.");
    hint(ui, "Resting glow is the light between hits; at 0 only the hits show.");
    match theme.as_str() {
        "ripple" => hint(ui, "Wave time is how long a wave takes to reach the top. Sparks at 0 turns them off."),
        "band" => hint(ui, "Spin is how fast the colors go round with the music; at 0 they stay put. Corners at 0 are square."),
        _ => {}
    }
}

/// The preview's frame time, 30 fps whatever the light runs at: each frame
/// redraws the whole window, about 3 ms of CPU on an Iris Xe.
const PREVIEW_FRAME: Duration = Duration::from_nanos(1_000_000_000 / 30);

/// A small screen with the theme's own shader on it, played by a made-up
/// beat. It plays only while this page shows and the window has the focus.
fn screen(ui: &mut Ui, view: &mut View, s: &Settings) {
    const BEZEL: f32 = 6.0;
    let width = ui.available_width().min(340.0);
    let height = (width - 2.0 * BEZEL) / view.aspect + 2.0 * BEZEL;
    let (outer, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let inner = outer.shrink(BEZEL);
    // The same dark monitor in the light and the dark window theme: the light
    // cannot be seen on a pale screen.
    let painter = ui.painter();
    painter.rect_filled(outer, 6.0, egui::Color32::from_rgb(0x2a, 0x2a, 0x2e));
    painter.rect_filled(inner, 0.0, egui::Color32::from_rgb(0x0b, 0x0b, 0x0d));

    let (source, params) = if view.playing {
        view.next_frame = Some(PREVIEW_FRAME);
        view.preview.step(s, Instant::now())
    } else {
        view.preview.still(s)
    };
    let ppp = ui.ctx().pixels_per_point();
    let uniforms = Uniforms {
        // The shader reads window pixels; this puts the preview's corner at 0, 0.
        origin: [-inner.min.x * ppp, -inner.min.y * ppp],
        screen: [inner.width() * ppp, inner.height() * ppp],
        base: view.colors.base,
        accent: view.colors.accent,
        time: view.preview.seconds(),
        thickness: 1.0,
        brightness: 1.0,
        resting: 1.0,
        params,
    }
    .with_look(view.preview.look());
    painter.add(egui_wgpu::Callback::new_paint_callback(inner, preview::Paint { source, uniforms }));
}

/// The palette's two colors as small dots.
fn swatch(ui: &mut Ui, name: &str) {
    let p = crate::palette(name);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(26.0, 14.0), egui::Sense::hover());
    let color = |c: [f32; 4]| egui::Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
    let painter = ui.painter();
    painter.circle_filled(rect.left_center() + egui::vec2(6.0, 0.0), 5.5, color(p.base));
    painter.circle_filled(rect.left_center() + egui::vec2(18.0, 0.0), 5.5, color(p.accent));
}

fn behavior(ui: &mut Ui, key: &mut KeyField, connected: &[Monitor], s: &mut Settings) {
    heading(ui, "Light");
    ui.checkbox(&mut s.enabled, "On");
    hint(ui, "The tray icon and the switch key turn it on and off too.");
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Frame rate");
        for fps in [60, 40, 30] {
            ui.radio_value(&mut s.fps, fps, fps.to_string());
        }
    });
    hint(ui, "Lower rates use less of the graphics card.");
    ui.add_space(4.0);
    ui.checkbox(&mut s.pause_on_fullscreen, "Hide during full screen");
    hint(ui, "Games and full screen video. Only the monitor they cover goes dark.");

    heading(ui, "Monitors");
    let lit: Vec<String> = monitors::lit(&s.monitors, connected).iter().map(|m| m.name.clone()).collect();
    for m in connected {
        let mut on = lit.contains(&m.name);
        // The last monitor with the light keeps it; the On switch turns it off.
        let last = on && lit.len() == 1;
        let label = if m.primary { format!("{} (main display)", m.name) } else { m.name.clone() };
        ui.horizontal(|ui| {
            if ui.add_enabled(!last, egui::Checkbox::new(&mut on, label)).changed() {
                s.monitors = monitors::choose(&s.monitors, connected, m, on);
            }
            ui.label(RichText::new(format!("{} × {}", m.width(), m.height())).small().weak());
        });
    }
    hint(ui, "The light frames each checked monitor on its own.");

    heading(ui, "Startup");
    ui.checkbox(&mut s.start_with_windows, "Start with Windows");

    heading(ui, "Switch key");
    ui.horizontal(|ui| {
        let label = if key.recording {
            "Press a key combination".to_string()
        } else if s.toggle_key.is_empty() {
            "None".to_string()
        } else {
            readable(&s.toggle_key)
        };
        let button = Button::new(label).selected(key.recording).min_size(egui::vec2(200.0, 28.0));
        if ui.add(button).clicked() {
            key.recording = !key.recording;
            key.note = None;
        }
        if ui.add_enabled(!s.toggle_key.is_empty() && !key.recording, Button::new("Clear")).clicked() {
            s.toggle_key.clear();
        }
    });
    if key.recording {
        record(ui, key, s);
    }
    match key.note {
        Some(note) => hint(ui, note),
        None if key.recording => hint(ui, "Hold Ctrl or Alt with a key. Esc cancels."),
        None => hint(ui, "Turns the light on and off from any app."),
    }
}

/// Takes the first key pressed with Ctrl or Alt as the new combination.
fn record(ui: &mut Ui, key: &mut KeyField, s: &mut Settings) {
    let pressed = ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
            _ => None,
        })
    });
    let Some((pressed, modifiers)) = pressed else { return };
    if pressed == egui::Key::Escape {
        key.recording = false;
        key.note = None;
        return;
    }
    match combination(pressed, modifiers) {
        Some(text) => {
            s.toggle_key = text;
            key.recording = false;
            key.note = None;
        }
        None => key.note = Some("That needs Ctrl or Alt, or the key cannot be used. Try another."),
    }
}

/// The settings file's text for a key pressed with its modifiers, like
/// "ctrl+alt+shift+l", if the hotkey library can register it.
fn combination(pressed: egui::Key, m: egui::Modifiers) -> Option<String> {
    if !(m.ctrl || m.alt) {
        return None;
    }
    let mut parts = Vec::new();
    for (held, name) in [(m.ctrl, "ctrl"), (m.alt, "alt"), (m.shift, "shift")] {
        if held {
            parts.push(name.to_string());
        }
    }
    parts.push(pressed.name().to_ascii_lowercase());
    let text = parts.join("+");
    settings::parse_key(&text).map(|_| text)
}

/// "ctrl+alt+shift+l" as "Ctrl + Alt + Shift + L".
fn readable(text: &str) -> String {
    text.split('+')
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "win" | "windows" | "super" => "Win".to_string(),
            p if p.chars().count() == 1 => p.to_uppercase(),
            p => capitalized(p),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, Modifiers};

    #[test]
    fn a_pressed_combination_reads_as_the_file_expects() {
        let ctrl_alt_shift = Modifiers { ctrl: true, alt: true, shift: true, command: true, ..Modifiers::NONE };
        let text = combination(Key::L, ctrl_alt_shift).unwrap();
        assert_eq!(text, settings::DEFAULT_TOGGLE_KEY);
        assert_eq!(settings::parse_key(&text), settings::parse_key(settings::DEFAULT_TOGGLE_KEY));
        let alt = Modifiers { alt: true, ..Modifiers::NONE };
        for k in [Key::F9, Key::Num5, Key::Space, Key::ArrowUp, Key::PageDown] {
            let text = combination(k, alt).unwrap_or_else(|| panic!("{k:?} was refused"));
            assert!(settings::parse_key(&text).is_some(), "{text}");
        }
        // A bare key or one with only Shift would fire while typing.
        assert_eq!(combination(Key::L, Modifiers::NONE), None);
        assert_eq!(combination(Key::L, Modifiers::SHIFT), None);
    }

    #[test]
    fn keys_read_the_way_they_are_printed() {
        assert_eq!(readable("ctrl+alt+shift+l"), "Ctrl + Alt + Shift + L");
        assert_eq!(readable("Win + Shift + F9"), "Win + Shift + F9");
    }
}

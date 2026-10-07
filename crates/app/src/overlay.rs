//! Transparent, click-through windows along the edges of each lit monitor and
//! the GPU state that draws the theme into them.

use std::sync::Arc;

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId, WindowLevel};

use crate::fullscreen::Rect;
use crate::settings::Look;
use crate::theme::Params;

/// How the light is laid out on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Four windows, each only as deep as the light can reach.
    Strips,
    /// One window covering the whole monitor.
    Full,
}

/// Matches `Uniforms` in theme/common.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub origin: [f32; 2],
    pub screen: [f32; 2],
    pub base: [f32; 4],
    pub accent: [f32; 4],
    pub time: f32,
    pub thickness: f32,
    pub brightness: f32,
    pub resting: f32,
    pub params: Params,
    pub shown: f32,
    pub _pad: [f32; 3],
}

struct Pane {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Where the window sits on its monitor, in the monitor's own pixels.
    origin: [f32; 2],
    /// Where the window belongs on the whole desktop, in physical pixels.
    place: (i32, i32, u32, u32),
    monitor: usize,
}

pub struct Overlay {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline_layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    panes: Vec<Pane>,
    /// Each monitor's size; every monitor gets its own frame.
    screens: Vec<[f32; 2]>,
    visible: Vec<bool>,
    /// Monitors switched visible that show once their first frame is drawn.
    waiting: Vec<bool>,
    /// How deep the strips were made, as a share of the shorter side.
    reach: f32,
}

/// `reach` is how deep the theme can draw, as a share of the shorter side.
fn rects(layout: Layout, width: u32, height: u32, reach: f32) -> Vec<(i32, i32, u32, u32)> {
    match layout {
        Layout::Full => vec![(0, 0, width, height)],
        Layout::Strips => {
            let depth = ((width.min(height) as f64 * reach as f64).ceil() as u32).min(height / 2);
            let side = height - 2 * depth;
            vec![
                (0, 0, width, depth),
                (0, (height - depth) as i32, width, depth),
                (0, depth as i32, depth, side),
                ((width - depth) as i32, depth as i32, depth, side),
            ]
        }
    }
}

impl Overlay {
    /// Opens the windows for `monitors`, all hidden until `set_visible`.
    pub fn new(event_loop: &ActiveEventLoop, monitors: &[Rect], layout: Layout, reach: f32, shader: &str) -> Self {

        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::DX12;
        // Only a swapchain composed through DirectComposition keeps per-pixel alpha on Windows.
        desc.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
        let instance = wgpu::Instance::new(desc);

        let size = |r: &Rect| ((r.2 - r.0).max(1) as u32, (r.3 - r.1).max(1) as u32);
        let windows: Vec<(Arc<Window>, [f32; 2], (i32, i32, u32, u32), usize)> = monitors
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                let (width, height) = size(m);
                rects(layout, width, height, reach).into_iter().map(move |(x, y, w, h)| (i, m.0 + x, m.1 + y, w, h, [x as f32, y as f32]))
            })
            .map(|(i, x, y, w, h, origin)| (create_window(event_loop, x, y, w, h), origin, (x, y, w, h), i))
            .collect();

        for (window, _, place, _) in &windows {
            put_back(window, *place);
        }
        let surfaces: Vec<wgpu::Surface<'static>> = windows
            .iter()
            .map(|(w, ..)| instance.create_surface(w.clone()).expect("creating a surface"))
            .collect();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: surfaces.first(),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("no DX12 adapter");
        eprintln!("gpu: {}", adapter.get_info().name);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("overlay"),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            ..Default::default()
        }))
        .expect("creating the device");

        let caps = surfaces[0].get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            eprintln!("warning: premultiplied alpha unavailable, got {:?}", caps.alpha_modes);
            caps.alpha_modes[0]
        };
        eprintln!("surface: {format:?}, {alpha_mode:?}");

        let layout_bg = uniform_layout(&device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout_bg)],
            immediate_size: 0,
        });
        let pipeline = build_pipeline(&device, &pipeline_layout, format, shader, None);

        let panes = windows
            .into_iter()
            .zip(surfaces)
            .map(|((window, origin, place, monitor), surface)| {
                // The window's own size, not what Windows made of it on a
                // monitor with another scale; `settle` has put it back already.
                let (w, h) = (place.2, place.3);
                let mut config = surface
                    .get_default_config(&adapter, w.max(1), h.max(1))
                    .expect("surface is supported by the adapter");
                config.format = format;
                config.alpha_mode = alpha_mode;
                config.present_mode = wgpu::PresentMode::Fifo;
                config.desired_maximum_frame_latency = 1;
                surface.configure(&device, &config);
                let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: std::mem::size_of::<Uniforms>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &layout_bg,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
                });
                Pane { window, surface, config, uniform, bind_group, origin, place, monitor }
            })
            .collect();

        let screens = monitors
            .iter()
            .map(|m| {
                let (w, h) = size(m);
                [w as f32, h as f32]
            })
            .collect();
        let overlay = Overlay {
            device,
            queue,
            pipeline_layout,
            format,
            pipeline,
            panes,
            screens,
            visible: vec![false; monitors.len()],
            waiting: vec![false; monitors.len()],
            reach,
        };
        overlay
    }

    /// How deep the windows reach, as they were built.
    pub fn reach(&self) -> f32 {
        self.reach
    }

    pub fn owns(&self, id: WindowId) -> bool {
        self.panes.iter().any(|p| p.window.id() == id)
    }

    /// Puts every window back on its own spot at its own size. Windows moves
    /// and scales a window whose monitor changes scale; the light is laid out
    /// in physical pixels and must not follow.
    pub fn settle(&self) {
        for pane in &self.panes {
            put_back(&pane.window, pane.place);
        }
    }

    /// Puts the shown windows back above the other always-on-top windows. The
    /// taskbar is one of them and rises over the light whenever it is used.
    pub fn raise(&self) {
        for pane in self.panes.iter().filter(|p| self.visible[p.monitor]) {
            platform::keep_on_top(&pane.window);
        }
    }

    /// Shows the light on the monitors marked true and hides it on the rest.
    /// Returns true when a monitor was switched on: it appears with the next
    /// `draw`, so it never shows a stale frame.
    pub fn set_visible(&mut self, visible: &[bool]) -> bool {
        let mut appeared = false;
        for (i, &on) in visible.iter().enumerate().take(self.visible.len()) {
            if on == self.visible[i] {
                continue;
            }
            self.visible[i] = on;
            self.waiting[i] = on;
            appeared |= on;
            if !on {
                for pane in self.panes.iter().filter(|p| p.monitor == i) {
                    pane.window.set_visible(false);
                }
            }
        }
        appeared
    }

    /// Swaps in another theme's shader without touching the windows.
    pub fn set_shader(&mut self, shader: &str) {
        self.pipeline = build_pipeline(&self.device, &self.pipeline_layout, self.format, shader, None);
    }

    pub fn draw(&mut self, uniforms: Uniforms) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut frames = Vec::with_capacity(self.panes.len());
        for pane in &mut self.panes {
            if !self.visible[pane.monitor] {
                continue;
            }
            let frame = match pane.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    pane.surface.configure(&self.device, &pane.config);
                    continue;
                }
                _ => continue,
            };
            let mut u = uniforms;
            u.origin = pane.origin;
            u.screen = self.screens[pane.monitor];
            self.queue.write_buffer(&pane.uniform, 0, bytemuck::bytes_of(&u));
            let view = frame.texture.create_view(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &pane.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            frames.push((pane.window.clone(), frame));
        }
        self.queue.submit([encoder.finish()]);
        for (window, frame) in frames {
            window.pre_present_notify();
            self.queue.present(frame);
        }
        for pane in &self.panes {
            if self.waiting[pane.monitor] {
                platform::show_without_focus(&pane.window);
                platform::keep_on_top(&pane.window);
            }
        }
        self.waiting.iter_mut().for_each(|w| *w = false);
    }
}

impl Uniforms {
    /// The user's thickness, brightness and resting glow for the theme drawn.
    pub fn with_look(mut self, look: &Look) -> Self {
        self.thickness = look.thickness;
        self.brightness = look.brightness;
        self.resting = look.resting;
        self
    }
}

/// The single uniform buffer every theme shader reads.
pub fn uniform_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

/// A theme shader's pipeline. The overlay replaces what is on screen
/// (`blend: None`); the settings preview blends over its dark screen.
pub fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    source: &str,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("theme"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("theme"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Moves and sizes `window` to `place` if it is anywhere else.
fn put_back(window: &Window, (x, y, w, h): (i32, i32, u32, u32)) {
    let at = window.outer_position().ok();
    let size = window.inner_size();
    if at != Some(PhysicalPosition::new(x, y)) || size != PhysicalSize::new(w, h) {
        eprintln!("overlay: window at {at:?} {}x{}, moved back to ({x}, {y}) {w}x{h}", size.width, size.height);
        window.set_outer_position(PhysicalPosition::new(x, y));
        let _ = window.request_inner_size(PhysicalSize::new(w, h));
    }
}

fn create_window(event_loop: &ActiveEventLoop, x: i32, y: i32, w: u32, h: u32) -> Arc<Window> {
    #[allow(unused_mut)]
    let mut attrs = Window::default_attributes()
        .with_title("beatframe")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_active(false)
        .with_visible(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_position(PhysicalPosition::new(x, y))
        .with_inner_size(PhysicalSize::new(w, h));
    #[cfg(windows)]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs = attrs
            .with_no_redirection_bitmap(true)
            .with_skip_taskbar(true)
            .with_undecorated_shadow(false)
            .with_drag_and_drop(false);
    }
    let window = Arc::new(event_loop.create_window(attrs).expect("creating an overlay window"));
    // Clicks and the cursor pass through to whatever is underneath.
    window.set_cursor_hittest(false).expect("click-through is supported");
    platform::never_take_focus(&window);
    window
}

#[cfg(windows)]
mod platform {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    fn hwnd(window: &Window) -> windows_sys::Win32::Foundation::HWND {
        match window.window_handle().expect("window handle").as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as _,
            _ => unreachable!("win32 window"),
        }
    }

    pub fn never_take_focus(window: &Window) {
        let hwnd = hwnd(window);
        // Tool windows stay out of Alt+Tab; no-activate keeps focus where the user left it.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize);
        }
    }

    pub fn show_without_focus(window: &Window) {
        unsafe {
            ShowWindow(hwnd(window), SW_SHOWNOACTIVATE);
        }
    }

    /// Always-on-top windows stack by which rose last; this makes the light the last.
    pub fn keep_on_top(window: &Window) {
        unsafe {
            SetWindowPos(hwnd(window), HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use winit::window::Window;

    pub fn never_take_focus(_window: &Window) {}

    pub fn keep_on_top(_window: &Window) {}

    pub fn show_without_focus(window: &Window) {
        window.set_visible(true);
    }
}

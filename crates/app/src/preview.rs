//! The small screen on the settings window's Look page: the chosen theme's own
//! shader, driven by a made-up rock beat so it moves without any music.

use std::time::Instant;

use crate::overlay::{self, Uniforms};
use crate::settings::Settings;
use crate::signal::Snapshot;
use crate::theme::{self, Params, Theme};

/// One bar of 4/4 at 120 BPM.
const BAR: f32 = 2.0;
/// Seconds into the bar and strength of each hit. Kick on 1, 3 and the "and"
/// of 3, snare on 2 and 4, hi-hat on every eighth. Strengths stay inside what
/// the analysis gives real drums, 0.35 to 1.
const KICK: [(f32, f32); 3] = [(0.0, 0.9), (1.0, 0.85), (1.25, 0.6)];
const SNARE: [(f32, f32); 2] = [(0.5, 0.8), (1.5, 0.8)];
const HAT: [(f32, f32); 8] =
    [(0.0, 0.6), (0.25, 0.45), (0.5, 0.6), (0.75, 0.45), (1.0, 0.6), (1.25, 0.45), (1.5, 0.6), (1.75, 0.45)];
/// About -16 dBFS, loud enough that the themes' loudness term is well up.
const LEVEL: f32 = 0.15;

/// What the audio thread would report `t` seconds into the beat.
fn beat(t: f32) -> Snapshot {
    let bars = (t / BAR).floor();
    let within = t - bars * BAR;
    let mut s = Snapshot { level: LEVEL, ..Default::default() };
    for (i, hits) in [&KICK[..], &SNARE[..], &HAT[..]].into_iter().enumerate() {
        let played: Vec<_> = hits.iter().filter(|(at, _)| *at <= within).collect();
        s.hits[i] = bars as u32 * hits.len() as u32 + played.len() as u32;
        // Before the bar's first hit, the last one heard closed the bar before.
        s.strength[i] = played.last().copied().unwrap_or(&hits[hits.len() - 1]).1;
    }
    s
}

/// The preview's own copy of the theme, kept apart from the one on the
/// screen edge so neither disturbs the other.
pub struct Preview {
    theme: Box<dyn Theme>,
    /// Seconds of the beat played so far; stands still while paused.
    clock: f32,
    /// When the last frame was played, or `None` after a pause.
    last: Option<Instant>,
}

impl Preview {
    pub fn new(settings: &Settings) -> Self {
        Preview {
            theme: theme::create(&settings.theme, settings).expect("settings only hold known themes"),
            clock: 0.0,
            last: None,
        }
    }

    fn follow(&mut self, settings: &Settings) {
        if self.theme.name() != settings.theme {
            if let Some(t) = theme::create(&settings.theme, settings) {
                self.theme = t;
            }
        } else {
            self.theme.configure(settings);
        }
    }

    /// Follows the settings' theme, plays the beat on to `now` and returns the
    /// values its shader needs.
    pub fn step(&mut self, settings: &Settings, now: Instant) -> (&'static str, Params) {
        self.follow(settings);
        // Capped so one slow frame does not skip a whole hit.
        let dt = self.last.map_or(0.0, |last| (now - last).as_secs_f32().min(0.1));
        self.last = Some(now);
        self.clock += dt;
        self.theme.update(&beat(self.clock), dt);
        (self.theme.source(), self.theme.params())
    }

    /// The frame it stopped on, for a window that is drawn while paused.
    pub fn still(&mut self, settings: &Settings) -> (&'static str, Params) {
        self.follow(settings);
        (self.theme.source(), self.theme.params())
    }

    /// Stops the beat where it is; the next `step` carries on from there.
    pub fn pause(&mut self) {
        self.last = None;
    }

    pub fn seconds(&self) -> f32 {
        self.clock % 1000.0
    }
}

/// GPU state for the preview, kept in the egui renderer's callback storage.
pub struct Gpu {
    format: wgpu::TextureFormat,
    layout: wgpu::PipelineLayout,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The pipeline and the theme source it was built from.
    pipeline: Option<(&'static str, wgpu::RenderPipeline)>,
}

impl Gpu {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bind_layout = overlay::uniform_layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("preview"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("preview"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        Gpu { format, layout, uniform, bind_group, pipeline: None }
    }
}

/// One frame of the preview, handed to egui as a paint callback.
pub struct Paint {
    /// The theme's own WGSL, `Theme::source`.
    pub source: &'static str,
    pub uniforms: Uniforms,
}

impl egui_wgpu::CallbackTrait for Paint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(gpu) = resources.get_mut::<Gpu>() else { return Vec::new() };
        if gpu.pipeline.as_ref().is_none_or(|(source, _)| *source != self.source) {
            let shader = theme::shader_for(self.source);
            let blend = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
            gpu.pipeline = Some((self.source, overlay::build_pipeline(device, &gpu.layout, gpu.format, &shader, blend)));
        }
        queue.write_buffer(&gpu.uniform, 0, bytemuck::bytes_of(&self.uniforms));
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(Gpu { pipeline: Some((_, pipeline)), bind_group, .. }) = resources.get::<Gpu>() else { return };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 120 BPM with three kicks, two snares and eight hi-hats a bar is 90, 60
    /// and 240 hits a minute.
    #[test]
    fn the_beat_keeps_its_tempo() {
        assert_eq!(beat(59.999).hits, [90, 60, 240]);
        assert_eq!(beat(0.0).hits, [1, 0, 1]);
        let before = beat(0.49);
        let on_two = beat(0.5);
        assert_eq!(on_two.hits[1], before.hits[1] + 1, "snare lands on beat two");
        assert_eq!(on_two.hits[0], before.hits[0], "no kick on beat two");
        for t in [0.0, 0.3, 1.1, 1.9, 7.77] {
            assert!(beat(t).strength.iter().all(|s| (0.35..=1.0).contains(s)), "{t}: {:?}", beat(t).strength);
        }
    }

    /// Driven for a bar at 60 fps, every theme follows a change of theme in the
    /// settings and draws something other than what the same loudness without
    /// any drum would give.
    #[test]
    fn the_preview_plays_drums_on_the_chosen_theme() {
        let mut settings = Settings::default();
        for name in theme::NAMES {
            settings.theme = name.to_string();
            let mut p = Preview::new(&Settings::default());
            let mut no_drums = theme::create(name, &settings).unwrap();
            let start = Instant::now();
            let (mut differed, mut moved, mut last) = (0, 0, None);
            let frame_time = std::time::Duration::from_micros(16_667);
            for frame in 1..=120 {
                let now = start + frame_time * frame;
                let (source, params) = p.step(&settings, now);
                assert_eq!(source, no_drums.source());
                no_drums.update(&Snapshot { level: LEVEL, ..Default::default() }, frame_time.as_secs_f32());
                if params != no_drums.params() {
                    differed += 1;
                }
                if last.is_some_and(|l| l != params) {
                    moved += 1;
                }
                last = Some(params);
            }
            assert_eq!(p.theme.name(), name);
            assert!(differed > 60, "{name}: drums showed in {differed} of 120 frames");
            // Hits land on 8 frames a bar; the rest move only if the light fades.
            assert!(moved > 100, "{name}: changed on {moved} of 119 frames");
        }
    }

    /// Away from the window for ten seconds, the preview comes back on the
    /// frame it left and plays on from there.
    #[test]
    fn a_paused_preview_carries_on_where_it_stopped() {
        let settings = Settings::default();
        let mut p = Preview::new(&settings);
        let start = Instant::now();
        let frame_time = std::time::Duration::from_millis(33);
        for frame in 0..40 {
            p.step(&settings, start + frame_time * frame);
        }
        let (clock, stopped) = (p.clock, p.still(&settings).1);
        p.pause();
        let back = start + frame_time * 40 + std::time::Duration::from_secs(10);
        assert_eq!(p.step(&settings, back).1, stopped);
        assert_eq!(p.clock, clock);
        p.step(&settings, back + frame_time);
        assert!((p.clock - clock - 0.033).abs() < 1e-4, "clock {} after {clock}", p.clock);
    }
}

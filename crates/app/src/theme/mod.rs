//! Themes decide what the light does with the drums. Each one pairs a WGSL
//! `light` function with the state it feeds that function every frame.

mod aurora;
mod band;
mod layered;
mod ripple;
mod split;

use crate::settings::{Drum, Look, Settings};
use crate::signal::Snapshot;

pub const NAMES: [&str; 5] = ["band", "layered", "split", "ripple", "aurora"];

/// Theme-specific uniform values, read in WGSL as `u.params`.
pub type Params = [[f32; 4]; 8];

pub trait Theme {
    fn name(&self) -> &'static str;

    /// Deepest the light can reach at the default thickness, as a share of the
    /// shorter screen side.
    fn depth(&self) -> f32;

    /// The user's look for this theme: thickness, brightness, drums, fade, resting glow.
    fn look(&self) -> &Look;

    /// Deepest the light can reach, as a share of the shorter screen side. The
    /// edge strips are made this deep, so drawing past it is cut off.
    fn reach(&self) -> f32 {
        self.depth() * self.look().thickness
    }

    /// WGSL that defines `fn light(e: Edge) -> Light`; see common.wgsl.
    fn source(&self) -> &'static str;

    fn update(&mut self, s: &Snapshot, dt: f32);

    /// True once nothing moves any more and drawing can stop.
    fn settled(&self) -> bool;

    /// Drops everything back to the resting frame.
    fn reset(&mut self);

    fn params(&self) -> Params;

    /// Takes new options from `settings` without dropping what is on screen.
    fn configure(&mut self, settings: &Settings);
}

/// Builds the named theme with its options from `settings`.
pub fn create(name: &str, settings: &Settings) -> Option<Box<dyn Theme>> {
    match name {
        "layered" => Some(Box::new(layered::Layered::new(&settings.layered))),
        "split" => Some(Box::new(split::Split::new(&settings.split))),
        "ripple" => Some(Box::new(ripple::Ripple::new(&settings.ripple))),
        "aurora" => Some(Box::new(aurora::Aurora::new(&settings.aurora))),
        "band" => Some(Box::new(band::Band::new(&settings.band))),
        _ => None,
    }
}

pub fn shader(theme: &dyn Theme) -> String {
    shader_for(theme.source())
}

/// The full shader around a theme's own `light` function.
pub fn shader_for(source: &str) -> String {
    format!("{}\n{}", include_str!("common.wgsl"), source)
}

const ENERGY_RISE: f32 = 0.05;
const ENERGY_FALL: f32 = 0.4;
/// Below this an envelope is invisible.
const SETTLED: f32 = 0.003;

/// Per-drum envelopes that jump on a hit and decay at frame rate, plus a
/// smoothed loudness.
pub struct Envelopes {
    pub drums: [f32; 3],
    pub energy: f32,
    fades: [f32; 3],
    /// The user's factor on every fade.
    fade: f32,
    /// Which drums count, and how strongly.
    answer: [Drum; 3],
    seen: [u32; 3],
}

impl Envelopes {
    /// `fades` is how long kick, snare and hi-hat take to fade to about a
    /// third at the default fade; `look` says which drums count and how much.
    pub fn new(fades: [f32; 3], look: &Look) -> Self {
        Envelopes { drums: [0.0; 3], energy: 0.0, fades, fade: look.fade, answer: look.drums, seen: [0; 3] }
    }

    pub fn set_look(&mut self, look: &Look) {
        self.fade = look.fade;
        self.answer = look.drums;
    }

    /// Returns the strength of each drum that hit since the last call, after
    /// the user's strength; a drum switched off never hits.
    pub fn update(&mut self, s: &Snapshot, dt: f32) -> [Option<f32>; 3] {
        let mut hits = [None; 3];
        for i in 0..3 {
            self.drums[i] *= (-dt / (self.fades[i] * self.fade)).exp();
            if s.hits[i] != self.seen[i] {
                self.seen[i] = s.hits[i];
                if self.answer[i].on {
                    let strength = s.strength[i] * self.answer[i].strength;
                    self.drums[i] = self.drums[i].max(strength);
                    hits[i] = Some(strength);
                }
            }
        }
        // -40 dBFS reads as silence, -6 dBFS as full.
        let db = 20.0 * s.level.max(1e-6).log10();
        let target = ((db + 40.0) / 34.0).clamp(0.0, 1.0);
        let tau = if target > self.energy { ENERGY_RISE } else { ENERGY_FALL };
        self.energy += (target - self.energy) * (1.0 - (-dt / tau).exp());
        hits
    }

    pub fn settled(&self) -> bool {
        self.drums.iter().all(|&d| d < SETTLED) && self.energy < SETTLED
    }

    pub fn reset(&mut self) {
        self.drums = [0.0; 3];
        self.energy = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::naga;

    #[test]
    fn every_name_creates_its_theme() {
        for name in NAMES {
            assert_eq!(create(name, &Settings::default()).map(|t| t.name()), Some(name));
        }
        assert!(create("nope", &Settings::default()).is_none());
    }

    /// Every value a theme takes from the settings: its own look and its own options.
    fn changed() -> Settings {
        let mut s = Settings::default();
        for name in NAMES {
            let look = s.look_mut(name);
            look.thickness = 1.5;
            look.brightness = 0.6;
            look.drums[0].strength = 0.5;
            look.drums[2].on = !look.drums[2].on;
            look.fade = 2.0;
            look.resting = 0.3;
        }
        s.layered.shimmer = 0.2;
        s.ripple.wave_seconds = 1.4;
        s.ripple.tail = 2.5;
        s.ripple.sparks = 0.0;
        s.aurora.flow = 2.0;
        s.aurora.folds = 1.5;
        s.band.spin = 2.0;
        s.band.waves = 0.5;
        s.band.corners = 1.0;
        s
    }

    /// A made-up run of drums, the same for every theme, four seconds long.
    fn drive(theme: &mut dyn Theme) -> Vec<Params> {
        let mut out = Vec::new();
        for frame in 0..240u32 {
            // Kick ten times a second, hi-hat twenty, snare once.
            let n = frame / 6;
            let s = Snapshot { hits: [n, frame / 60, n * 2], strength: [0.9, 0.7, 0.5], level: 0.12 };
            theme.update(&s, 1.0 / 60.0);
            out.push(theme.params());
        }
        out
    }

    /// The settings window moves sliders live, so each change reconfigures the
    /// theme in place instead of building it again.
    #[test]
    fn configuring_in_place_matches_building_anew() {
        let changed = changed();
        for name in NAMES {
            assert_ne!(changed.look(name), Settings::default().look(name), "{name}");
            let mut theme = create(name, &Settings::default()).unwrap();
            theme.configure(&changed);
            let mut fresh = create(name, &changed).unwrap();
            assert_eq!(theme.look(), fresh.look(), "{name}");
            assert_eq!(theme.reach(), fresh.reach(), "{name}");
            assert_eq!(drive(theme.as_mut()), drive(fresh.as_mut()), "{name}");
        }
    }

    /// Every option shows in what the theme hands the shader, so none is
    /// read from the file and then ignored.
    #[test]
    fn every_option_changes_the_light() {
        let changed = changed();
        for name in NAMES {
            let mut before = create(name, &Settings::default()).unwrap();
            let mut after = create(name, &changed).unwrap();
            assert_ne!(drive(before.as_mut()), drive(after.as_mut()), "{name}");
            assert!(after.reach() > before.reach(), "{name}: thicker light reaches deeper");
        }
        // The theme's own options one at a time, against its defaults.
        let one = |edit: fn(&mut Settings), name: &str| {
            let mut s = Settings::default();
            edit(&mut s);
            let mut a = create(name, &Settings::default()).unwrap();
            let mut b = create(name, &s).unwrap();
            assert_ne!(drive(a.as_mut()), drive(b.as_mut()), "{name}");
        };
        one(|s| s.layered.shimmer = 0.2, "layered");
        one(|s| s.ripple.wave_seconds = 1.4, "ripple");
        one(|s| s.ripple.tail = 2.5, "ripple");
        one(|s| s.ripple.sparks = 0.0, "ripple");
        one(|s| s.aurora.flow = 2.0, "aurora");
        one(|s| s.aurora.folds = 1.5, "aurora");
        one(|s| s.band.spin = 2.0, "band");
        one(|s| s.band.waves = 0.5, "band");
        one(|s| s.band.corners = 1.0, "band");
        // Thickness, brightness and resting glow go to the shader beside the
        // params, through `look`; fade and the drums shape the params.
        for name in NAMES {
            for edit in [|l: &mut Look| l.fade = 2.0, |l: &mut Look| l.drums[0].strength = 0.3] {
                let mut s = Settings::default();
                edit(s.look_mut(name));
                let mut a = create(name, &Settings::default()).unwrap();
                let mut b = create(name, &s).unwrap();
                assert_ne!(drive(a.as_mut()), drive(b.as_mut()), "{name}");
                assert_eq!(b.look(), s.look(name), "{name}");
            }
        }
    }

    #[test]
    fn a_drum_switched_off_never_hits() {
        let mut look = Look::default();
        look.drums[1].on = false;
        look.drums[0].strength = 0.5;
        let mut env = Envelopes::new([0.2, 0.1, 0.05], &look);
        let hits = env.update(&Snapshot { hits: [1, 1, 1], strength: [0.8, 0.8, 0.8], level: 0.1 }, 0.016);
        assert_eq!(hits, [Some(0.4), None, Some(0.8)]);
        assert_eq!(env.drums[1], 0.0);
    }

    /// Thickness and brightness are applied once for every theme; the resting
    /// glow is each theme's own, so each shader has to read it.
    #[test]
    fn every_shader_reads_the_look() {
        let common = include_str!("common.wgsl");
        let body = &common[common.find("fn fs(").expect("the fragment shader")..];
        assert!(body.contains("u.thickness") && body.contains("u.brightness"), "{body}");
        for name in NAMES {
            let theme = create(name, &Settings::default()).unwrap();
            assert!(theme.source().contains("u.resting"), "{name} has no resting glow");
        }
    }

    /// Only Band draws a solid strip; every other theme stays under the shared
    /// cap, so its light never turns into a solid band over the screen.
    #[test]
    fn only_band_goes_past_the_peak_cap() {
        for name in NAMES {
            let source = create(name, &Settings::default()).unwrap().source();
            let capped = source.contains(", PEAK);");
            assert_eq!(capped, name != "band", "{name}");
        }
    }

    #[test]
    fn every_theme_shader_validates() {
        for name in NAMES {
            let src = shader(create(name, &Settings::default()).unwrap().as_ref());
            let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        }
    }

    /// The Rust struct is copied byte for byte into the WGSL one, so both must
    /// agree on size and on where each value sits.
    #[test]
    fn uniform_layout_matches_the_shader() {
        use crate::overlay::Uniforms;

        let src = shader(create(NAMES[0], &Settings::default()).unwrap().as_ref());
        let module = naga::front::wgsl::parse_str(&src).unwrap();
        let (members, span) = module
            .types
            .iter()
            .find_map(|(_, t)| match (&t.name, &t.inner) {
                (Some(n), naga::TypeInner::Struct { members, span }) if n == "Uniforms" => Some((members, *span)),
                _ => None,
            })
            .expect("Uniforms in common.wgsl");
        let offset = |name: &str| members.iter().find(|m| m.name.as_deref() == Some(name)).map(|m| m.offset as usize);
        assert_eq!(span as usize, std::mem::size_of::<Uniforms>());
        assert_eq!(offset("params"), Some(std::mem::offset_of!(Uniforms, params)));
        assert_eq!(offset("thickness"), Some(std::mem::offset_of!(Uniforms, thickness)));
        assert_eq!(offset("brightness"), Some(std::mem::offset_of!(Uniforms, brightness)));
        assert_eq!(offset("resting"), Some(std::mem::offset_of!(Uniforms, resting)));
    }
}

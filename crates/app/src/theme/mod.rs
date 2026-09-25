//! Themes decide what the light does with the drums. Each one pairs a WGSL
//! `light` function with the state it feeds that function every frame.

mod layered;
mod ripple;
mod split;

use crate::settings::Settings;
use crate::signal::Snapshot;

pub const NAMES: [&str; 3] = ["layered", "split", "ripple"];

/// Theme-specific uniform values, read in WGSL as `u.params`.
pub type Params = [[f32; 4]; 8];

pub trait Theme {
    fn name(&self) -> &'static str;

    /// Deepest the light can reach, as a share of the shorter screen side. The
    /// edge strips are made this deep, so drawing past it is cut off.
    fn reach(&self) -> f32;

    /// WGSL that defines `fn light(e: Edge) -> Light`; see common.wgsl.
    fn source(&self) -> &'static str;

    fn update(&mut self, s: &Snapshot, dt: f32);

    /// True once nothing moves any more and drawing can stop.
    fn settled(&self) -> bool;

    /// Drops everything back to the resting frame.
    fn reset(&mut self);

    fn params(&self) -> Params;

    /// Takes new options from `settings` without dropping what is on screen.
    fn configure(&mut self, _settings: &Settings) {}
}

/// Builds the named theme with its options from `settings`.
pub fn create(name: &str, settings: &Settings) -> Option<Box<dyn Theme>> {
    match name {
        "layered" => Some(Box::new(layered::Layered::new())),
        "split" => Some(Box::new(split::Split::new(settings.split.quiet_edge))),
        "ripple" => Some(Box::new(ripple::Ripple::new(&settings.ripple))),
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
    seen: [u32; 3],
}

impl Envelopes {
    /// `fades` is how long kick, snare and hi-hat take to fade to about a third.
    pub fn new(fades: [f32; 3]) -> Self {
        Envelopes { drums: [0.0; 3], energy: 0.0, fades, seen: [0; 3] }
    }

    /// Returns the strength of each drum that hit since the last call.
    pub fn update(&mut self, s: &Snapshot, dt: f32) -> [Option<f32>; 3] {
        let mut hits = [None; 3];
        for i in 0..3 {
            self.drums[i] *= (-dt / self.fades[i]).exp();
            if s.hits[i] != self.seen[i] {
                self.seen[i] = s.hits[i];
                self.drums[i] = self.drums[i].max(s.strength[i]);
                hits[i] = Some(s.strength[i]);
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

    /// The settings window moves sliders live, so each change reconfigures the
    /// theme in place instead of building it again.
    #[test]
    fn configuring_in_place_matches_building_anew() {
        use crate::settings::{QuietEdge, RippleSettings, SplitSettings};

        let changed = Settings {
            split: SplitSettings { quiet_edge: QuietEdge::Off },
            ripple: RippleSettings { wave_seconds: 1.4, tail: 2.5, sparks: 0.0 },
            ..Settings::default()
        };
        assert_ne!(changed.split, Settings::default().split);
        assert_ne!(changed.ripple, Settings::default().ripple);
        for name in NAMES {
            let mut theme = create(name, &Settings::default()).unwrap();
            theme.configure(&changed);
            assert_eq!(theme.params(), create(name, &changed).unwrap().params(), "{name}");
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
    /// agree on size and on where the theme values start.
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
        let params = members.iter().find(|m| m.name.as_deref() == Some("params")).expect("params member");
        assert_eq!(span as usize, std::mem::size_of::<Uniforms>());
        assert_eq!(params.offset as usize, std::mem::offset_of!(Uniforms, params));
    }
}

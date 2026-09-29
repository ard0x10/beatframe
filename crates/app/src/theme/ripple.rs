use super::{Envelopes, Params, Theme};
use crate::settings::{Look, RippleSettings, Settings};
use crate::signal::Snapshot;

/// Waves the shader draws at once; a ninth pushes out the oldest.
const MAX_WAVES: usize = 8;

pub struct Ripple {
    env: Envelopes,
    look: Look,
    /// (age in seconds, strength), oldest first.
    waves: Vec<(f32, f32)>,
    /// Seconds a wave takes from the bottom middle to the top middle.
    travel: f32,
    tail: f32,
    sparks: f32,
}

impl Ripple {
    pub fn new(options: &RippleSettings) -> Self {
        Ripple {
            // Kick, snare, hi-hat: seconds to fade to about a third. The kick
            // envelope is unused here; each kick starts a wave instead.
            env: Envelopes::new([0.22, 0.12, 0.07], &options.look),
            look: options.look.clone(),
            waves: Vec::with_capacity(MAX_WAVES),
            travel: options.wave_seconds,
            tail: options.tail,
            sparks: options.sparks,
        }
    }
}

impl Theme for Ripple {
    fn name(&self) -> &'static str {
        "ripple"
    }

    fn depth(&self) -> f32 {
        0.12
    }

    fn look(&self) -> &Look {
        &self.look
    }

    fn source(&self) -> &'static str {
        include_str!("ripple.wgsl")
    }

    fn update(&mut self, s: &Snapshot, dt: f32) {
        let hits = self.env.update(s, dt);
        for w in &mut self.waves {
            w.0 += dt;
        }
        let travel = self.travel;
        self.waves.retain(|w| w.0 < travel);
        if let Some(strength) = hits[0] {
            if self.waves.len() == MAX_WAVES {
                self.waves.remove(0);
            }
            self.waves.push((0.0, strength));
        }
    }

    fn settled(&self) -> bool {
        self.waves.is_empty() && self.env.settled()
    }

    fn reset(&mut self) {
        self.env.reset();
        self.waves.clear();
    }

    fn configure(&mut self, settings: &Settings) {
        let options = &settings.ripple;
        self.env.set_look(&options.look);
        self.look = options.look.clone();
        self.travel = options.wave_seconds;
        self.tail = options.tail;
        self.sparks = options.sparks;
    }

    fn params(&self) -> Params {
        let [_, snare, hat] = self.env.drums;
        let mut p = Params::default();
        p[0] = [snare, hat, self.env.energy, self.travel];
        p[5] = [self.tail, self.sparks, 0.0, 0.0];
        for (i, &(age, strength)) in self.waves.iter().enumerate() {
            let slot = &mut p[1 + i / 2];
            let at = (i % 2) * 2;
            slot[at] = age;
            slot[at + 1] = strength;
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ripple() -> Ripple {
        Ripple::new(&crate::settings::Settings::default().ripple)
    }

    fn kick(hits: u32) -> Snapshot {
        Snapshot { hits: [hits, 0, 0], strength: [0.8, 0.0, 0.0], level: 0.1 }
    }

    #[test]
    fn each_kick_starts_a_wave_that_ends_after_its_travel() {
        let mut r = ripple();
        r.update(&kick(1), 0.016);
        r.update(&kick(2), 0.016);
        assert_eq!(r.waves.len(), 2);
        assert_eq!(r.params()[1][1], 0.8);
        for _ in 0..60 {
            r.update(&kick(2), 0.016);
        }
        assert!(r.waves.is_empty(), "waves still alive after {} s: {:?}", r.travel, r.waves);
    }

    #[test]
    fn a_ninth_wave_pushes_out_the_oldest() {
        let mut r = ripple();
        for n in 1..=9 {
            r.update(&kick(n), 0.01);
        }
        assert_eq!(r.waves.len(), MAX_WAVES);
        // The oldest left is the second kick, 7 frames old.
        assert!((r.waves[0].0 - 0.07).abs() < 1e-6, "{:?}", r.waves[0]);
    }
}

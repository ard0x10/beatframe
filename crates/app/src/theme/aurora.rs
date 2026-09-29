use super::{Envelopes, Params, Theme};
use crate::settings::{AuroraSettings, Look, Settings};
use crate::signal::Snapshot;

/// Radians the curtain flows a second at full loudness and the default flow.
const FLOW: f32 = 1.6;
/// Share of that it keeps while the music is quiet but not silent.
const QUIET_FLOW: f32 = 0.25;

pub struct Aurora {
    env: Envelopes,
    look: Look,
    /// Where the flow is; it only moves forward, and stands still in silence.
    phase: f32,
    flow: f32,
    folds: f32,
}

impl Aurora {
    pub fn new(options: &AuroraSettings) -> Self {
        Aurora {
            // Kick, snare, hi-hat: seconds to fade to about a third, softer than the other themes.
            env: Envelopes::new([0.35, 0.25, 0.1], &options.look),
            look: options.look.clone(),
            phase: 0.0,
            flow: options.flow,
            folds: options.folds,
        }
    }
}

impl Theme for Aurora {
    fn name(&self) -> &'static str {
        "aurora"
    }

    fn depth(&self) -> f32 {
        0.12
    }

    fn look(&self) -> &Look {
        &self.look
    }

    fn source(&self) -> &'static str {
        include_str!("aurora.wgsl")
    }

    fn update(&mut self, s: &Snapshot, dt: f32) {
        let _ = self.env.update(s, dt);
        let pace = QUIET_FLOW + (1.0 - QUIET_FLOW) * self.env.energy;
        // Kept small so the shader's sines stay precise over hours of music. The
        // shader runs it at 1, 1.7, 0.6 and 0.8 times; a hundred turns is a whole
        // number of turns for each, so the wrap does not show.
        self.phase = (self.phase + dt * FLOW * self.flow * pace) % (std::f32::consts::TAU * 100.0);
    }

    fn settled(&self) -> bool {
        self.env.settled()
    }

    fn reset(&mut self) {
        // The curtain keeps where it had flowed to, so it does not jump back.
        self.env.reset();
    }

    fn configure(&mut self, settings: &Settings) {
        let options = &settings.aurora;
        self.env.set_look(&options.look);
        self.look = options.look.clone();
        self.flow = options.flow;
        self.folds = options.folds;
    }

    fn params(&self) -> Params {
        let [kick, snare, hat] = self.env.drums;
        let mut p = Params::default();
        p[0] = [kick, snare, hat, self.env.energy];
        p[1] = [self.phase, self.folds, 0.0, 0.0];
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curtain_flows_faster_with_loud_music_and_stops_in_silence() {
        let flowed = |level: f32| {
            let mut a = Aurora::new(&Settings::default().aurora);
            let s = Snapshot { level, ..Default::default() };
            for _ in 0..120 {
                a.update(&s, 1.0 / 60.0);
            }
            a.phase
        };
        let (loud, quiet, silent) = (flowed(0.5), flowed(0.02), flowed(0.0));
        assert!(loud > 2.0 * quiet, "{loud} against {quiet}");
        // Silence still moves it at the quiet pace until the drawing stops.
        assert!(silent > 0.0 && silent <= quiet);
    }

    /// Ten seconds of loud music at twice the flow is 2 x 1.6 x 10 radians.
    #[test]
    fn flow_scales_the_pace() {
        let mut a = Aurora::new(&AuroraSettings { flow: 2.0, ..Settings::default().aurora });
        let s = Snapshot { level: 1.0, ..Default::default() };
        for _ in 0..600 {
            a.update(&s, 1.0 / 60.0);
        }
        // Energy rises within a fraction of a second.
        assert!((a.phase - 32.0).abs() < 1.0, "{}", a.phase);
    }
}

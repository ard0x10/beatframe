use super::{Envelopes, Params, Theme};
use crate::settings::QuietEdge;
use crate::signal::Snapshot;

pub struct Split {
    env: Envelopes,
    quiet: QuietEdge,
}

impl Split {
    pub fn new(quiet: QuietEdge) -> Self {
        // Kick, snare, hi-hat: seconds to fade to about a third.
        Split { env: Envelopes::new([0.22, 0.12, 0.07]), quiet }
    }
}

impl Theme for Split {
    fn name(&self) -> &'static str {
        "split"
    }

    fn reach(&self) -> f32 {
        0.12
    }

    fn source(&self) -> &'static str {
        include_str!("split.wgsl")
    }

    fn update(&mut self, s: &Snapshot, dt: f32) {
        let _ = self.env.update(s, dt);
    }

    fn settled(&self) -> bool {
        self.env.settled()
    }

    fn reset(&mut self) {
        self.env.reset();
    }

    fn configure(&mut self, settings: &crate::settings::Settings) {
        self.quiet = settings.split.quiet_edge;
    }

    fn params(&self) -> Params {
        let [kick, snare, hat] = self.env.drums;
        let mut p = Params::default();
        p[0] = [kick, snare, hat, self.env.energy];
        p[1][0] = match self.quiet {
            QuietEdge::Dim => 1.0,
            QuietEdge::Off => 0.0,
        };
        p
    }
}

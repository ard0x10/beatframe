use super::{Envelopes, Params, Theme};
use crate::signal::Snapshot;

pub struct Layered {
    env: Envelopes,
}

impl Layered {
    pub fn new() -> Self {
        // Kick, snare, hi-hat: seconds to fade to about a third.
        Layered { env: Envelopes::new([0.22, 0.12, 0.07]) }
    }
}

impl Theme for Layered {
    fn name(&self) -> &'static str {
        "layered"
    }

    fn reach(&self) -> f32 {
        0.12
    }

    fn source(&self) -> &'static str {
        include_str!("layered.wgsl")
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

    fn params(&self) -> Params {
        let [kick, snare, hat] = self.env.drums;
        let mut p = Params::default();
        p[0] = [kick, snare, hat, self.env.energy];
        p
    }
}

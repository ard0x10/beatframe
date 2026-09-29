use super::{Envelopes, Params, Theme};
use crate::settings::{LayeredSettings, Look, Settings};
use crate::signal::Snapshot;

pub struct Layered {
    env: Envelopes,
    look: Look,
    shimmer: f32,
}

impl Layered {
    pub fn new(options: &LayeredSettings) -> Self {
        // Kick, snare, hi-hat: seconds to fade to about a third.
        Layered { env: Envelopes::new([0.22, 0.12, 0.07], &options.look), look: options.look.clone(), shimmer: options.shimmer }
    }
}

impl Theme for Layered {
    fn name(&self) -> &'static str {
        "layered"
    }

    fn depth(&self) -> f32 {
        0.12
    }

    fn look(&self) -> &Look {
        &self.look
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

    fn configure(&mut self, settings: &Settings) {
        let options = &settings.layered;
        self.env.set_look(&options.look);
        self.look = options.look.clone();
        self.shimmer = options.shimmer;
    }

    fn params(&self) -> Params {
        let [kick, snare, hat] = self.env.drums;
        let mut p = Params::default();
        p[0] = [kick, snare, hat, self.env.energy];
        p[1][0] = self.shimmer;
        p
    }
}

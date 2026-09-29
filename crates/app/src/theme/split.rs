use super::{Envelopes, Params, Theme};
use crate::settings::{Look, Settings, SplitSettings};
use crate::signal::Snapshot;

pub struct Split {
    env: Envelopes,
    look: Look,
}

impl Split {
    pub fn new(options: &SplitSettings) -> Self {
        // Kick, snare, hi-hat: seconds to fade to about a third.
        Split { env: Envelopes::new([0.22, 0.12, 0.07], &options.look), look: options.look.clone() }
    }
}

impl Theme for Split {
    fn name(&self) -> &'static str {
        "split"
    }

    fn depth(&self) -> f32 {
        0.12
    }

    fn look(&self) -> &Look {
        &self.look
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

    fn configure(&mut self, settings: &Settings) {
        self.env.set_look(&settings.split.look);
        self.look = settings.split.look.clone();
    }

    fn params(&self) -> Params {
        let [kick, snare, hat] = self.env.drums;
        let mut p = Params::default();
        p[0] = [kick, snare, hat, self.env.energy];
        p
    }
}

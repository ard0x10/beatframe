use std::f32::consts::TAU;

use super::{Envelopes, Params, Theme};
use crate::settings::{BandSettings, Look, Settings};
use crate::signal::Snapshot;

/// Radians a second the colors turn at the default spin: while the music is
/// quiet, on top of that at full loudness, and at the height of a kick.
const QUIET_TURN: f32 = 0.35;
const LOUD_TURN: f32 = 0.9;
const KICK_TURN: f32 = 2.5;
/// Radians a second the inner edge's waves move, the same three ways.
const QUIET_STIR: f32 = 0.8;
const LOUD_STIR: f32 = 1.5;
const HAT_STIR: f32 = 3.0;

pub struct Band {
    env: Envelopes,
    look: Look,
    /// How far the colors have turned round the frame.
    turn: f32,
    /// Where the waves of the inner edge are.
    stir: f32,
    spin: f32,
    waves: f32,
    corners: f32,
}

impl Band {
    pub fn new(options: &BandSettings) -> Self {
        Band {
            // Kick, snare, hi-hat: seconds to fade to about a third.
            env: Envelopes::new([0.25, 0.15, 0.08], &options.look),
            look: options.look.clone(),
            turn: 0.0,
            stir: 0.0,
            spin: options.spin,
            waves: options.waves,
            corners: options.corners,
        }
    }
}

impl Theme for Band {
    fn name(&self) -> &'static str {
        "band"
    }

    fn depth(&self) -> f32 {
        // The band at its widest: a full kick and the deepest wave.
        0.18
    }

    fn look(&self) -> &Look {
        &self.look
    }

    fn source(&self) -> &'static str {
        include_str!("band.wgsl")
    }

    fn update(&mut self, s: &Snapshot, dt: f32) {
        let _ = self.env.update(s, dt);
        let [kick, _, hat] = self.env.drums;
        let energy = self.env.energy;
        let turning = QUIET_TURN + LOUD_TURN * energy + KICK_TURN * kick.min(1.5);
        self.turn = (self.turn + dt * self.spin * turning) % TAU;
        // The shader runs the stir at 1, 1.3, 0.7, 0.5 and 0.8 times; ten
        // turns is a whole number of turns for each, so the wrap does not show.
        let stirring = QUIET_STIR + LOUD_STIR * energy + HAT_STIR * hat.min(1.0);
        self.stir = (self.stir + dt * stirring) % (TAU * 10.0);
    }

    fn settled(&self) -> bool {
        self.env.settled()
    }

    fn reset(&mut self) {
        // The colors stay where the music left them.
        self.env.reset();
    }

    fn configure(&mut self, settings: &Settings) {
        let options = &settings.band;
        self.env.set_look(&options.look);
        self.look = options.look.clone();
        self.spin = options.spin;
        self.waves = options.waves;
        self.corners = options.corners;
    }

    fn params(&self) -> Params {
        let [kick, snare, hat] = self.env.drums;
        let mut p = Params::default();
        p[0] = [kick, snare, hat, self.env.energy];
        p[1] = [self.turn, self.stir, self.waves, self.corners];
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How far the colors turn in two seconds of `level` with a kick every
    /// `kick_every` frames, or none.
    fn turned(spin: f32, level: f32, kick_every: Option<u32>) -> f32 {
        let mut b = Band::new(&BandSettings { spin, ..Settings::default().band });
        let mut total = 0.0;
        for frame in 0..120u32 {
            let kicks = kick_every.map_or(0, |k| frame / k);
            let before = b.turn;
            b.update(&Snapshot { hits: [kicks, 0, 0], strength: [0.9, 0.0, 0.0], level }, 1.0 / 60.0);
            total += (b.turn - before).rem_euclid(TAU);
        }
        total
    }

    #[test]
    fn the_colors_turn_with_the_music() {
        let quiet = turned(1.0, 0.0, None);
        let loud = turned(1.0, 0.5, None);
        let kicked = turned(1.0, 0.5, Some(30));
        // Silence turns them at the quiet pace alone: 0.35 rad/s for two seconds.
        assert!((quiet - 0.7).abs() < 0.01, "{quiet}");
        assert!(loud > 2.5 * quiet, "{loud} against {quiet}");
        assert!(kicked > 1.3 * loud, "{kicked} against {loud}");
        assert_eq!(turned(0.0, 0.5, Some(30)), 0.0, "spin 0 keeps the colors still");
    }

    #[test]
    fn a_long_song_keeps_the_numbers_small() {
        let mut b = Band::new(&BandSettings { spin: 3.0, ..Settings::default().band });
        let loud = Snapshot { hits: [0, 0, 0], strength: [0.0; 3], level: 1.0 };
        for _ in 0..60 * 60 * 10 {
            b.update(&loud, 1.0 / 60.0);
        }
        assert!((0.0..TAU).contains(&b.turn), "{}", b.turn);
        assert!((0.0..TAU * 10.0).contains(&b.stir), "{}", b.stir);
    }
}

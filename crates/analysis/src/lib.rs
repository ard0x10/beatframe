//! Turns a mono audio stream into per-frame drum signals.
//!
//! Loudness alone hides drums in dense mixes: a heavily mastered rock track keeps
//! its overall level almost flat while the kick and snare hit. Each drum lives in
//! its own frequency range, so the spectrum is split into bands and every band is
//! watched for sudden rises (spectral flux) instead of level.

use std::collections::VecDeque;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

pub const FFT_SIZE: usize = 2048;
pub const HOP: usize = 512;

const LOG_GAIN: f32 = 1000.0;
const THRESHOLD_WINDOW_S: f64 = 0.5;
// Distorted guitars keep the band restless; a hit has to stand out from that restlessness.
// History keeps each value capped at the threshold it faced, so one hit does not hide the next.
const THRESHOLD_SPREAD: f32 = 2.5;
const THRESHOLD_FLOOR: f32 = 0.05;
const PULSE_RELEASE_S: f64 = 0.15;
// Rises are measured against the spectrum this many hops back, widened by one bin on
// each side, so wobbling bass and vibrato do not read as hits (SuperFlux, Boeck & Widmer 2013).
const RISE_LAG: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drum {
    Kick,
    Snare,
    Hat,
}

impl Drum {
    pub const ALL: [Drum; 3] = [Drum::Kick, Drum::Snare, Drum::Hat];

    pub fn name(self) -> &'static str {
        match self {
            Drum::Kick => "kick",
            Drum::Snare => "snare",
            Drum::Hat => "hat",
        }
    }

    fn ranges_hz(self) -> &'static [(f32, f32)] {
        match self {
            Drum::Kick => &[(40.0, 120.0)],
            // Body of the shell plus the crack of the wires.
            Drum::Snare => &[(150.0, 250.0), (1500.0, 5000.0)],
            Drum::Hat => &[(6000.0, 16000.0)],
        }
    }

    fn refractory_s(self) -> f64 {
        match self {
            Drum::Kick | Drum::Snare => 0.12,
            Drum::Hat => 0.06,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DrumFrame {
    /// Mean rectified rise of the log spectrum inside the band.
    pub flux: f32,
    /// Flux needed to count as a hit, adapted to the recent past.
    pub threshold: f32,
    pub onset: bool,
    /// 0..1, jumps on a hit and fades out. This is what visuals read.
    pub pulse: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Seconds from the first sample to the center of the analysis window.
    pub time: f64,
    /// RMS of the samples around the window center.
    pub level: f32,
    pub drums: [DrumFrame; 3],
}

impl Frame {
    pub fn drum(&self, drum: Drum) -> &DrumFrame {
        &self.drums[drum as usize]
    }
}

struct Band {
    drum: Drum,
    bins: Vec<usize>,
    history: VecDeque<f32>,
    history_sum: f32,
    history_sq_sum: f32,
    last_onset: f64,
    pulse: f32,
}

pub struct Analyzer {
    sample_rate: u32,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    magnitude_scale: f32,
    pending: Vec<f32>,
    frame_in: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    past_logs: VecDeque<Vec<f32>>,
    widened: Vec<f32>,
    rise: Vec<f32>,
    bands: Vec<Band>,
    history_len: usize,
    pulse_decay: f32,
    frames_done: u64,
}

impl Analyzer {
    pub fn new(sample_rate: u32) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                let phase = 2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32;
                0.5 - 0.5 * phase.cos()
            })
            .collect();
        // A full-scale sine then reads as magnitude 1.
        let magnitude_scale = 2.0 / window.iter().sum::<f32>();
        let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
        let bin_count = FFT_SIZE / 2 + 1;

        let bands = Drum::ALL
            .iter()
            .map(|&drum| {
                let bins = (0..bin_count)
                    .filter(|&k| {
                        let hz = k as f32 * bin_hz;
                        drum.ranges_hz().iter().any(|&(lo, hi)| hz >= lo && hz < hi)
                    })
                    .collect();
                Band {
                    drum,
                    bins,
                    history: VecDeque::new(),
                    history_sum: 0.0,
                    history_sq_sum: 0.0,
                    last_onset: f64::NEG_INFINITY,
                    pulse: 0.0,
                }
            })
            .collect();

        let hop_s = HOP as f64 / sample_rate as f64;
        Analyzer {
            sample_rate,
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
            magnitude_scale,
            pending: Vec::with_capacity(FFT_SIZE * 2),
            frame_in: vec![0.0; FFT_SIZE],
            past_logs: VecDeque::with_capacity(RISE_LAG + 1),
            widened: vec![0.0; bin_count],
            rise: vec![0.0; bin_count],
            bands,
            history_len: (THRESHOLD_WINDOW_S / hop_s).round().max(1.0) as usize,
            pulse_decay: (-hop_s / PULSE_RELEASE_S).exp() as f32,
            frames_done: 0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Feeds mono samples and appends one frame per completed hop to `out`.
    pub fn push(&mut self, samples: &[f32], out: &mut Vec<Frame>) {
        self.pending.extend_from_slice(samples);
        let mut start = 0;
        while self.pending.len() - start >= FFT_SIZE {
            out.push(self.analyze(start));
            start += HOP;
        }
        self.pending.drain(..start);
    }

    fn analyze(&mut self, start: usize) -> Frame {
        let block = &self.pending[start..start + FFT_SIZE];
        for ((dst, &x), &w) in self.frame_in.iter_mut().zip(block).zip(&self.window) {
            *dst = x * w;
        }

        let center = FFT_SIZE / 2;
        let around_center = &block[center - HOP / 2..center + HOP / 2];
        let level = (around_center.iter().map(|x| x * x).sum::<f32>() / HOP as f32).sqrt();

        self.fft
            .process_with_scratch(&mut self.frame_in, &mut self.spectrum, &mut self.scratch)
            .expect("buffer sizes come from the same plan");

        let time = (self.frames_done * HOP as u64 + center as u64) as f64 / self.sample_rate as f64;
        self.frames_done += 1;

        let mut drums = [DrumFrame::default(); 3];
        let mut log = if self.past_logs.len() > RISE_LAG {
            self.past_logs.pop_front().unwrap_or_default()
        } else {
            vec![0.0; self.spectrum.len()]
        };
        for (l, c) in log.iter_mut().zip(&self.spectrum) {
            *l = (1.0 + LOG_GAIN * c.norm() * self.magnitude_scale).ln();
        }
        if self.past_logs.len() == RISE_LAG {
            let reference = &self.past_logs[0];
            let last = reference.len() - 1;
            for k in 0..=last {
                self.widened[k] = reference[k.saturating_sub(1)].max(reference[k]).max(reference[(k + 1).min(last)]);
            }
            for ((rise, &now), &before) in self.rise.iter_mut().zip(&log).zip(&self.widened) {
                *rise = (now - before).max(0.0);
            }
        } else {
            self.rise.fill(0.0);
        }
        self.past_logs.push_back(log);

        for band in &mut self.bands {
            let flux = if band.bins.is_empty() {
                0.0
            } else {
                band.bins.iter().map(|&k| self.rise[k]).sum::<f32>() / band.bins.len() as f32
            };

            let n = band.history.len().max(1) as f32;
            let mean = band.history_sum / n;
            let spread = (band.history_sq_sum / n - mean * mean).max(0.0).sqrt();
            let threshold = mean + THRESHOLD_SPREAD * spread + THRESHOLD_FLOOR;
            let onset = flux > threshold && time - band.last_onset >= band.drum.refractory_s();

            band.pulse *= self.pulse_decay;
            if onset {
                band.last_onset = time;
                let strength = ((flux - threshold) / threshold).clamp(0.0, 1.0);
                band.pulse = band.pulse.max(0.35 + 0.65 * strength);
            }

            let kept = flux.min(threshold);
            band.history.push_back(kept);
            band.history_sum += kept;
            band.history_sq_sum += kept * kept;
            if band.history.len() > self.history_len {
                let old = band.history.pop_front().unwrap_or(0.0);
                band.history_sum -= old;
                band.history_sq_sum -= old * old;
            }

            drums[band.drum as usize] = DrumFrame { flux, threshold, onset, pulse: band.pulse };
        }

        Frame { time, level, drums }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    struct Noise(u64);

    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 23) as f32 - 1.0
        }
    }

    /// Distorted, heavily compressed chord bed with drums on a known grid.
    fn rock_like(seconds: f64, with_kick: bool) -> (Vec<f32>, Vec<f64>) {
        let n = (seconds * RATE as f64) as usize;
        let mut out = vec![0.0f32; n];
        let mut noise = Noise(0x9E37_79B9_7F4A_7C15);

        for (i, s) in out.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            let chord: f32 = [110.0f32, 164.8, 220.0, 329.6]
                .iter()
                .map(|f| 2.0 * (t * f).fract() - 1.0)
                .sum();
            *s = (3.0 * chord).tanh() * 0.35 + noise.next() * 0.05;
        }

        let beat = 0.5;
        let mut kicks = Vec::new();
        let mut t = 0.25;
        while t + 0.3 < seconds {
            if with_kick {
                kicks.push(t);
                let start = (t * RATE as f64) as usize;
                for j in 0..(0.2 * RATE as f64) as usize {
                    let tt = j as f32 / RATE as f32;
                    let freq = 50.0 + 60.0 * (-tt / 0.03).exp();
                    let phase = 2.0 * std::f32::consts::PI * freq * tt;
                    out[start + j] += 0.9 * phase.sin() * (-tt / 0.08).exp();
                }
            }
            let hat = ((t + beat / 2.0) * RATE as f64) as usize;
            let mut last = 0.0;
            for j in 0..(0.04 * RATE as f64) as usize {
                let x = noise.next();
                let tt = j as f32 / RATE as f32;
                out[hat + j] += 0.4 * (x - last) * (-tt / 0.01).exp();
                last = x;
            }
            t += beat;
        }

        // Mastering-style limiter keeps the overall level nearly flat.
        for s in &mut out {
            *s = s.tanh();
        }
        (out, kicks)
    }

    fn kick_onsets(signal: &[f32]) -> Vec<f64> {
        let mut analyzer = Analyzer::new(RATE);
        let mut frames = Vec::new();
        for chunk in signal.chunks(480) {
            analyzer.push(chunk, &mut frames);
        }
        frames.iter().filter(|f| f.drum(Drum::Kick).onset).map(|f| f.time).collect()
    }

    #[test]
    fn finds_kicks_buried_in_a_dense_mix() {
        let (signal, truth) = rock_like(10.0, true);
        let found = kick_onsets(&signal);
        assert!(truth.len() >= 18, "grid produced {} kicks", truth.len());

        let tolerance = 0.03;
        let hits = truth.iter().filter(|&&t| found.iter().any(|&f| (f - t).abs() <= tolerance)).count();
        let stray = found.iter().filter(|&&f| !truth.iter().any(|&t| (f - t).abs() <= tolerance)).count();

        assert!(hits as f64 >= truth.len() as f64 * 0.9, "hit {hits} of {} kicks", truth.len());
        assert!(stray <= 1, "{stray} onsets away from any kick: {found:?}");
    }

    #[test]
    fn stays_quiet_when_the_kick_is_absent() {
        let (signal, truth) = rock_like(10.0, false);
        assert!(truth.is_empty());
        let found = kick_onsets(&signal);
        assert!(found.len() <= 2, "{} kick onsets without a kick: {found:?}", found.len());
    }
}

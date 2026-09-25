//! Carries drum hits from the audio thread to the render loop.
//!
//! The audio thread only counts hits and stores numbers; how long a hit glows is
//! up to the theme, which decays its own envelopes at frame rate.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use analysis::{Analyzer, Drum, Frame};

/// Below this RMS the output counts as silent and the overlay may stop drawing.
pub const WAKE_LEVEL: f32 = 0.003;

/// Wait before opening the output again after capture failed, for example
/// while Windows is still bringing up audio at sign-in.
const RETRY: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Shared {
    hits: [AtomicU32; 3],
    strength: [AtomicU32; 3],
    level: AtomicU32,
    /// Set by the render loop when it stops drawing; the audio thread wakes it.
    pub idle: AtomicBool,
    /// Set while the light is switched off; samples are dropped unanalysed.
    pub paused: AtomicBool,
}

#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub hits: [u32; 3],
    pub strength: [f32; 3],
    pub level: f32,
}

impl Shared {
    pub fn snapshot(&self) -> Snapshot {
        let mut s = Snapshot { level: f32::from_bits(self.level.load(Ordering::Relaxed)), ..Default::default() };
        for i in 0..3 {
            s.hits[i] = self.hits[i].load(Ordering::Acquire);
            s.strength[i] = f32::from_bits(self.strength[i].load(Ordering::Relaxed));
        }
        s
    }

    fn publish(&self, frame: &Frame) -> bool {
        let mut hit = false;
        for drum in Drum::ALL {
            let d = frame.drum(drum);
            if d.onset {
                let i = drum as usize;
                self.strength[i].store(d.pulse.to_bits(), Ordering::Relaxed);
                self.hits[i].fetch_add(1, Ordering::Release);
                hit = true;
            }
        }
        self.level.store(frame.level.to_bits(), Ordering::Relaxed);
        hit || frame.level > WAKE_LEVEL
    }
}

/// Starts loopback capture and analysis. `wake` is called when sound returns
/// while the render loop is idle. Capture follows the default output and is
/// opened again whenever it fails.
pub fn spawn(shared: Arc<Shared>, wake: impl Fn() + Send + 'static) {
    thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            loop {
                match listen(&shared, &wake) {
                    Ok(capture::Ended::Stopped) => return,
                    Ok(capture::Ended::DeviceChanged) => eprintln!("audio: default output changed, following it"),
                    Err(e) => {
                        eprintln!("audio capture stopped: {e}; retrying in {} s", RETRY.as_secs());
                        thread::sleep(RETRY);
                    }
                }
            }
        })
        .expect("spawning the audio thread");
}

/// One capture run on the current default output. The analyzer starts fresh,
/// since another device may run at another sample rate.
fn listen(shared: &Shared, wake: &impl Fn()) -> Result<capture::Ended, capture::Error> {
    let mut analyzer: Option<Analyzer> = None;
    let mut frames = Vec::new();
    let mut busy = Duration::ZERO;
    let mut window_start = Instant::now();
    capture::run_loopback(|rate, samples| {
        if shared.paused.load(Ordering::Acquire) {
            return true;
        }
        let started = Instant::now();
        let a = analyzer.get_or_insert_with(|| Analyzer::new(rate));
        frames.clear();
        a.push(samples, &mut frames);
        let mut active = false;
        for f in &frames {
            active |= shared.publish(f);
        }
        if active && shared.idle.swap(false, Ordering::AcqRel) {
            wake();
        }
        busy += started.elapsed();
        if window_start.elapsed().as_secs() >= 5 {
            let share = busy.as_secs_f64() / window_start.elapsed().as_secs_f64() * 100.0;
            eprintln!("audio: analysis {share:.2}% of one core");
            busy = Duration::ZERO;
            window_start = Instant::now();
        }
        true
    })
}

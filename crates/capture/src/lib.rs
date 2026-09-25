//! Captures what the default output device is playing, as mono f32 samples.

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Why a capture run returned.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// The sink returned false.
    Stopped,
    /// Another output became the default; call again to follow it.
    DeviceChanged,
}

/// Blocks and hands mono samples to `sink` until it returns false or the
/// default output changes.
///
/// While nothing plays, Windows delivers no packets; silence is emitted every
/// 50 ms instead so downstream time keeps moving.
pub fn run_loopback(sink: impl FnMut(u32, &[f32]) -> bool) -> Result<Ended, Error> {
    platform::run_loopback(sink)
}

#[cfg(windows)]
mod platform {
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat, initialize_mta};

    use super::{Ended, Error};

    const QUIET_WAIT_MS: u32 = 50;
    /// A loopback stream stays on the device it opened, so the default output
    /// is looked up again on this interval.
    const DEFAULT_CHECK: Duration = Duration::from_secs(2);

    pub fn run_loopback(mut sink: impl FnMut(u32, &[f32]) -> bool) -> Result<Ended, Error> {
        // Already initialized on this thread is fine.
        let _ = initialize_mta();

        let enumerator = DeviceEnumerator::new()?;
        let device = enumerator.get_default_device(&Direction::Render)?;
        let id = device.get_id()?;
        let mut checked = Instant::now();
        let mut client = device.get_iaudioclient()?;
        let rate = client.get_mixformat()?.get_samplespersec();
        let format = WaveFormat::new(32, 32, &SampleType::Float, rate as usize, 2, None);
        let (_, min_period) = client.get_device_period()?;
        client.initialize_client(
            &format,
            &Direction::Capture,
            &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: min_period },
        )?;
        let event = client.set_get_eventhandle()?;
        let capture = client.get_audiocaptureclient()?;
        client.start_stream()?;

        let quiet = vec![0.0f32; (rate as usize * QUIET_WAIT_MS as usize) / 1000];
        let mut bytes = VecDeque::new();
        let mut mono = Vec::new();

        let ended = loop {
            if checked.elapsed() >= DEFAULT_CHECK {
                checked = Instant::now();
                let default = enumerator.get_default_device(&Direction::Render).and_then(|d| d.get_id());
                if default.is_ok_and(|default| default != id) {
                    break Ended::DeviceChanged;
                }
            }
            if event.wait_for_event(QUIET_WAIT_MS).is_err() {
                if !sink(rate, &quiet) {
                    break Ended::Stopped;
                }
                continue;
            }

            loop {
                let before = bytes.len();
                let info = capture.read_from_device_to_deque(&mut bytes)?;
                if bytes.len() == before {
                    break;
                }
                if info.flags.silent {
                    for b in bytes.range_mut(before..) {
                        *b = 0;
                    }
                }
            }

            mono.clear();
            let frame_bytes = 8;
            let whole = bytes.len() / frame_bytes * frame_bytes;
            let raw: Vec<u8> = bytes.drain(..whole).collect();
            for frame in raw.chunks_exact(frame_bytes) {
                let left = f32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]);
                let right = f32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
                mono.push(0.5 * (left + right));
            }
            if !mono.is_empty() && !sink(rate, &mono) {
                break Ended::Stopped;
            }
        };

        client.stop_stream()?;
        Ok(ended)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Ended, Error};

    pub fn run_loopback(_sink: impl FnMut(u32, &[f32]) -> bool) -> Result<Ended, Error> {
        Err("loopback capture is not implemented on this platform yet".into())
    }
}

use std::f32::consts::TAU;
use std::num::NonZero;
use std::sync::{Arc, OnceLock};

use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source, buffer::SamplesBuffer};

use crate::timer::{CompletionEvent, TimerStyle};

const SAMPLE_RATE: u32 = 48_000;

pub struct AudioEngine {
    sink: Option<MixerDeviceSink>,
    buffers: [SamplesBuffer; 3],
    warning: Arc<OnceLock<String>>,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioEngine {
    pub fn new() -> Self {
        // Generation happens only at initialization, not in UI or audio callbacks.
        let buffers = TimerStyle::ALL.map(synthesize);
        let warning = Arc::new(OnceLock::new());
        let callback_warning = Arc::clone(&warning);
        let result = DeviceSinkBuilder::from_default_device().and_then(|builder| {
            builder
                .with_error_callback(move |error| {
                    let _ = callback_warning.set(format!("Audio device failed: {error}"));
                })
                .open_stream()
        });
        let sink = match result {
            Ok(sink) => Some(sink),
            Err(error) => {
                let _ = warning.set(format!("Audio is unavailable: {error}"));
                None
            }
        };
        Self {
            sink,
            buffers,
            warning,
        }
    }

    pub fn play_completion(&mut self, event: CompletionEvent) {
        if event.volume <= 0.0 || !event.volume.is_finite() {
            return;
        }
        let Some(sink) = &self.sink else {
            return;
        };
        let index = match event.style {
            TimerStyle::Bomb => 0,
            TimerStyle::Hourglass => 1,
            TimerStyle::Rocket => 2,
        };
        // SamplesBuffer::clone shares its Arc-backed samples, with an independent cursor.
        // Adding directly to the mixer overlaps voices instead of queuing them.
        sink.mixer()
            .add(self.buffers[index].clone().amplify(event.volume.min(1.0)));
    }

    pub fn warning(&self) -> Option<&str> {
        // OnceLock lets the device thread report a permanent error without returning
        // a reference into a mutex guard or allocating during successful polling.
        self.warning.get().map(String::as_str)
    }
}

fn synthesize(style: TimerStyle) -> SamplesBuffer {
    let length_seconds = match style {
        TimerStyle::Bomb => 1.1,
        TimerStyle::Hourglass => 1.6,
        TimerStyle::Rocket => 1.35,
    };
    let length = (length_seconds * SAMPLE_RATE as f32) as usize;
    let mut samples = Vec::with_capacity(length);
    let mut noise_state = 0x4a91_f25d_u32;
    let mut filtered_noise = 0.0_f32;
    let mut phase = 0.0_f32;

    for index in 0..length {
        let time = index as f32 / SAMPLE_RATE as f32;
        let progress = index as f32 / (length - 1) as f32;
        let sample = match style {
            TimerStyle::Bomb => {
                let noise = next_noise(&mut noise_state);
                filtered_noise += 0.08 * (noise - filtered_noise);
                let frequency = 35.0 + 70.0 * (-7.0 * time).exp();
                phase = (phase + TAU * frequency / SAMPLE_RATE as f32) % TAU;
                let body = phase.sin() * (-4.5 * time).exp();
                let impact = filtered_noise * (-8.0 * time).exp();
                0.55 * body + 0.35 * impact
            }
            TimerStyle::Hourglass => {
                let mut chime = 0.0;
                for (onset, frequency, strength) in [
                    (0.0, 880.0, 0.42),
                    (0.14, 1174.66, 0.28),
                    (0.28, 1760.0, 0.18),
                ] {
                    if time >= onset {
                        let elapsed = time - onset;
                        let attack = (elapsed / 0.008).min(1.0);
                        chime += strength
                            * attack
                            * (-4.0 * elapsed).exp()
                            * ((TAU * frequency * elapsed).sin()
                                + 0.18 * (TAU * frequency * 2.01 * elapsed).sin());
                    }
                }
                chime
            }
            TimerStyle::Rocket => {
                let noise = next_noise(&mut noise_state);
                let cutoff = 0.025 + 0.5 * progress;
                filtered_noise += cutoff * (noise - filtered_noise);
                let frequency = 80.0 + 720.0 * progress * progress;
                phase = (phase + TAU * frequency / SAMPLE_RATE as f32) % TAU;
                // The envelope is nonnegative; f32 sin(PI) can round below zero.
                let envelope = (std::f32::consts::PI * progress).sin().max(0.0).powf(0.8);
                envelope * (0.6 * filtered_noise + 0.11 * phase.sin())
            }
        };
        // Short ramps prevent clicks; every preset ends at zero and cannot loop.
        let attack = (time / 0.004).min(1.0);
        let release = ((length - 1 - index) as f32 / (SAMPLE_RATE as f32 * 0.04)).min(1.0);
        samples.push((sample * attack * release).clamp(-0.75, 0.75));
    }

    SamplesBuffer::new(
        NonZero::new(1).expect("mono channel count is nonzero"),
        NonZero::new(SAMPLE_RATE).expect("sample rate is nonzero"),
        samples,
    )
}

fn next_noise(state: &mut u32) -> f32 {
    // Fixed-seed noise keeps presets deterministic without a random dependency.
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn presets_end_and_contain_finite_bounded_audible_samples() {
        for style in TimerStyle::ALL {
            let mut buffer = synthesize(style);
            let duration = buffer.total_duration().unwrap();
            assert!((Duration::from_secs(1)..=Duration::from_secs(2)).contains(&duration));
            let mut energy = 0.0_f64;
            let mut last = None;
            for sample in buffer.by_ref() {
                assert!(sample.is_finite());
                assert!(sample.abs() <= 0.75);
                energy += f64::from(sample * sample);
                last = Some(sample);
            }
            assert!(energy > 1.0, "{style:?} must not be a silent fallback");
            assert_eq!(last, Some(0.0));
            assert_eq!(buffer.next(), None);
            assert_eq!(buffer.next(), None);
        }
    }
}

use rayon::prelude::*;

use super::dsp::{PowerSpectrum, percentile};
use super::math::smoothstep;

const FFT_SIZE: usize = 2048;
const HOP_SIZE: usize = 256;
const KICK_MIN_FREQUENCY: f32 = 40.0;
const KICK_MAX_FREQUENCY: f32 = 300.0;
const MAGNITUDE_COMPRESSION: f32 = 100.0;
const FLUX_LAG_HOPS: usize = 2;

const PEAK_RADIUS_SECONDS: f32 = 0.05;
const CONTEXT_RADIUS_SECONDS: f32 = 1.5;
const RELATIVE_THRESHOLD: f32 = 0.35;
const MIN_ONSET_STRENGTH: f32 = 0.2;
const MIN_BEAT_INTERVAL_SECONDS: f32 = 0.18;

const STRENGTH_REFERENCE_PERCENTILE: f32 = 0.9;
const MIN_STRENGTH: f32 = 0.6;
const ATTACK_SECONDS: f32 = 0.05;
const DECAY_SECONDS: f32 = 0.2;
const DECAY_SPAN: f32 = 5.0;

struct Beat {
    time: f32,
    strength: f32,
}

pub struct BeatPulse {
    values: Vec<f32>,
    beat_count: usize,
}

impl BeatPulse {
    pub fn detect(samples: &[f32], sample_rate: u32, fps: u32, frame_count: usize) -> Self {
        let hops_per_second = sample_rate as f32 / HOP_SIZE as f32;
        let onsets = onset_strength(samples, sample_rate);
        let beats = pick_beats(&onsets, hops_per_second);
        Self {
            values: envelope(&beats, fps, frame_count),
            beat_count: beats.len(),
        }
    }

    pub fn beat_count(&self) -> usize {
        self.beat_count
    }

    pub fn at(&self, frame: usize) -> f32 {
        self.values.get(frame).copied().unwrap_or(0.0)
    }
}

fn onset_strength(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
    let first_bin = (KICK_MIN_FREQUENCY / bin_hz).round().max(2.0) as usize;
    let last_bin = (KICK_MAX_FREQUENCY / bin_hz).round() as usize;
    let magnitude_scale = MAGNITUDE_COMPRESSION * 4.0 / FFT_SIZE as f32;
    let analyzer = PowerSpectrum::new(FFT_SIZE);
    let hop_count = samples.len() / HOP_SIZE + 1;

    let spectra: Vec<Vec<f32>> = (0..hop_count)
        .into_par_iter()
        .map_init(
            || analyzer.clone(),
            |worker, hop| {
                let power = worker.compute(samples, (hop * HOP_SIZE) as isize);
                power[first_bin - 1..=last_bin + 1]
                    .iter()
                    .map(|bin| (bin.sqrt() * magnitude_scale).ln_1p())
                    .collect()
            },
        )
        .collect();

    let silence = vec![0.0; last_bin - first_bin + 3];
    (0..hop_count)
        .into_par_iter()
        .map(|hop| {
            let current = &spectra[hop];
            let previous = hop.checked_sub(FLUX_LAG_HOPS).map_or(&silence, |earlier| &spectra[earlier]);
            let rise: f32 = (1..current.len() - 1)
                .map(|bin| {
                    let reference = previous[bin - 1].max(previous[bin]).max(previous[bin + 1]);
                    (current[bin] - reference).max(0.0)
                })
                .sum();
            rise / (current.len() - 2) as f32
        })
        .collect()
}

fn pick_beats(onsets: &[f32], hops_per_second: f32) -> Vec<Beat> {
    let peak_radius = (PEAK_RADIUS_SECONDS * hops_per_second).round() as usize;
    let context_radius = (CONTEXT_RADIUS_SECONDS * hops_per_second).round() as usize;
    let min_gap = (MIN_BEAT_INTERVAL_SECONDS * hops_per_second).round() as usize;

    let mut peaks: Vec<(usize, f32)> = Vec::new();
    for (hop, &value) in onsets.iter().enumerate() {
        if value < MIN_ONSET_STRENGTH {
            continue;
        }
        let is_local_max = neighborhood(onsets, hop, peak_radius).iter().all(|&other| other <= value);
        if !is_local_max {
            continue;
        }
        let context_start = hop.saturating_sub(context_radius).max(FLUX_LAG_HOPS);
        let context_end = (hop + context_radius + 1).min(onsets.len());
        let context_peak = onsets
            .get(context_start..context_end)
            .unwrap_or_default()
            .iter()
            .copied()
            .fold(0.0, f32::max);
        if value < RELATIVE_THRESHOLD * context_peak {
            continue;
        }
        match peaks.last_mut() {
            Some(last) if hop - last.0 < min_gap => {
                if value > last.1 {
                    *last = (hop, value);
                }
            }
            _ => peaks.push((hop, value)),
        }
    }

    let strengths: Vec<f32> = peaks.iter().map(|&(_, value)| value).collect();
    let Some(strength_reference) = percentile(&strengths, STRENGTH_REFERENCE_PERCENTILE) else {
        return Vec::new();
    };
    peaks
        .into_iter()
        .map(|(hop, value)| Beat {
            time: ((hop as f32 - FLUX_LAG_HOPS as f32 / 2.0) / hops_per_second).max(0.0),
            strength: (value / strength_reference).min(1.0).sqrt().max(MIN_STRENGTH),
        })
        .collect()
}

fn neighborhood(values: &[f32], index: usize, radius: usize) -> &[f32] {
    &values[index.saturating_sub(radius)..(index + radius + 1).min(values.len())]
}

fn envelope(beats: &[Beat], fps: u32, frame_count: usize) -> Vec<f32> {
    let attack_frames = ATTACK_SECONDS * fps as f32;
    let decay_frames = DECAY_SECONDS * fps as f32;
    let last_frame = frame_count.saturating_sub(1);
    let mut values = vec![0.0_f32; frame_count];

    for beat in beats {
        let peak = (beat.time * fps as f32).round() as usize;
        let first = peak.saturating_sub(attack_frames.floor() as usize);
        let last = (peak + (decay_frames * DECAY_SPAN).ceil() as usize).min(last_frame);
        for (frame, value) in values.iter_mut().enumerate().take(last + 1).skip(first) {
            let offset = frame as f32 - peak as f32;
            let shape = if offset < 0.0 {
                smoothstep(0.0, 1.0, 1.0 + offset / attack_frames)
            } else {
                (-offset / decay_frames).exp()
            };
            *value = value.max(beat.strength * shape);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::BeatPulse;

    const SAMPLE_RATE: u32 = 44_100;
    const FPS: u32 = 30;
    const SECONDS: f32 = 8.0;
    const BEAT_INTERVAL: f32 = 0.6;

    fn synthesize(sample: impl Fn(f32) -> f32) -> Vec<f32> {
        let count = (SECONDS * SAMPLE_RATE as f32) as usize;
        (0..count).map(|index| sample(index as f32 / SAMPLE_RATE as f32)).collect()
    }

    fn frame_count() -> usize {
        (SECONDS * FPS as f32) as usize
    }

    fn kick_drum(since_kick: f32) -> f32 {
        let sweep = 0.03;
        let phase = TAU * (50.0 * since_kick + 130.0 * sweep * (1.0 - (-since_kick / sweep).exp()));
        0.9 * phase.sin() * (-since_kick * 9.0).exp()
    }

    #[test]
    fn pulse_peaks_on_each_kick_and_relaxes_between_kicks() {
        for (mix, bass_gain) in [("light bass", 0.1), ("loud sustained sub-bass", 0.5)] {
            let samples = synthesize(|time| {
                let bass = bass_gain * ((TAU * 49.0 * time).sin() + 0.5 * (TAU * 98.0 * time).sin());
                let pad = 0.1 * [261.6, 329.6, 392.0].iter().map(|f| (TAU * f * time).sin()).sum::<f32>();
                kick_drum(time % BEAT_INTERVAL) + bass + pad
            });

            let pulse = BeatPulse::detect(&samples, SAMPLE_RATE, FPS, frame_count());
            let kick_count = (SECONDS / BEAT_INTERVAL).ceil() as usize;
            assert_eq!(pulse.beat_count(), kick_count, "{mix}");

            let frames_per_beat = (BEAT_INTERVAL * FPS as f32).round() as usize;
            for kick in 0..kick_count {
                let kick_frame = kick * frames_per_beat;
                let window_start = kick_frame.saturating_sub(frames_per_beat / 2);
                let window_end = (kick_frame + frames_per_beat / 2).min(frame_count());
                let (peak_frame, peak) = (window_start..window_end)
                    .map(|frame| (frame, pulse.at(frame)))
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap();

                assert!(peak_frame.abs_diff(kick_frame) <= 1, "{mix}: kick {kick} peaked at frame {peak_frame}");
                assert!(peak >= 0.7, "{mix}: kick {kick} peaked at only {peak}");
                if window_end - kick_frame == frames_per_beat / 2 {
                    let between = pulse.at(kick_frame + frames_per_beat / 2);
                    assert!(between <= 0.3, "{mix}: pulse after kick {kick} stayed at {between}");
                }
            }
        }
    }

    #[test]
    fn swelling_pad_without_attacks_does_not_pulse() {
        let samples = synthesize(|time| {
            let fade_in = time.min(1.0);
            let swell = 0.75 + 0.25 * (TAU * 0.5 * time).sin();
            let tones = [65.4, 98.0, 130.8, 261.6].iter().map(|f| (TAU * f * time).sin()).sum::<f32>();
            0.15 * fade_in * swell * tones
        });

        let pulse = BeatPulse::detect(&samples, SAMPLE_RATE, FPS, frame_count());
        assert_eq!(pulse.beat_count(), 0);
        assert!((0..frame_count()).all(|frame| pulse.at(frame) == 0.0));
    }
}

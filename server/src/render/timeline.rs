use super::audio::Spectrum;
use super::beats::BeatPulse;
use super::dsp::percentile;
use super::math::smoothstep;

const FADE_IN_SECONDS: f32 = 1.0;
const FADE_OUT_SECONDS: f32 = 1.5;
const ENERGY_FLOOR_PERCENTILE: f32 = 0.1;
const ENERGY_CEILING_PERCENTILE: f32 = 0.95;
const ENERGY_ATTACK: f32 = 0.3;
const ENERGY_RELEASE: f32 = 0.06;
const ENERGY_SPEEDUP: f32 = 1.5;
const BEAT_SPEEDUP: f32 = 2.5;

pub struct FrameInput<'a> {
    pub levels: &'a [f32],
    pub beat: f32,
    pub energy: f32,
    pub travel: f32,
    pub seconds: f32,
    pub brightness: f32,
}

pub struct Timeline {
    fps: u32,
    spectrum: Spectrum,
    beats: BeatPulse,
    energy: Vec<f32>,
    travel: Vec<f32>,
}

impl Timeline {
    pub fn analyze(samples: &[f32], sample_rate: u32, fps: u32, bar_count: usize) -> Self {
        let spectrum = Spectrum::analyze(samples, sample_rate, fps, bar_count);
        let beats = BeatPulse::detect(samples, sample_rate, fps, spectrum.frame_count());
        let energy = energy_envelope(&spectrum);
        let travel = travel_distance(&energy, &beats, fps);
        Self {
            fps,
            spectrum,
            beats,
            energy,
            travel,
        }
    }

    pub fn frame_count(&self) -> usize {
        self.spectrum.frame_count()
    }

    pub fn beat_count(&self) -> usize {
        self.beats.beat_count()
    }

    pub fn frame(&self, index: usize) -> FrameInput<'_> {
        let seconds = index as f32 / self.fps as f32;
        let last_frame_seconds = self.frame_count().saturating_sub(1) as f32 / self.fps as f32;
        let fade_in = smoothstep(0.0, FADE_IN_SECONDS, seconds);
        let fade_out = smoothstep(last_frame_seconds, last_frame_seconds - FADE_OUT_SECONDS, seconds);
        FrameInput {
            levels: self.spectrum.frame(index),
            beat: self.beats.at(index),
            energy: self.energy[index],
            travel: self.travel[index],
            seconds,
            brightness: fade_in.min(fade_out),
        }
    }
}

fn energy_envelope(spectrum: &Spectrum) -> Vec<f32> {
    let loudness: Vec<f32> = (0..spectrum.frame_count())
        .map(|index| {
            let levels = spectrum.frame(index);
            levels.iter().sum::<f32>() / levels.len() as f32
        })
        .collect();

    let floor = percentile(&loudness, ENERGY_FLOOR_PERCENTILE).unwrap_or(0.0);
    let ceiling = percentile(&loudness, ENERGY_CEILING_PERCENTILE).unwrap_or(1.0);
    let span = (ceiling - floor).max(f32::EPSILON);

    let mut smoothed = 0.0;
    loudness
        .iter()
        .map(|value| {
            let target = ((value - floor) / span).clamp(0.0, 1.0);
            let rate = if target > smoothed { ENERGY_ATTACK } else { ENERGY_RELEASE };
            smoothed += (target - smoothed) * rate;
            smoothed
        })
        .collect()
}

fn travel_distance(energy: &[f32], beats: &BeatPulse, fps: u32) -> Vec<f32> {
    let mut distance = 0.0;
    energy
        .iter()
        .enumerate()
        .map(|(index, level)| {
            let current = distance;
            distance += (1.0 + ENERGY_SPEEDUP * level + BEAT_SPEEDUP * beats.at(index)) / fps as f32;
            current
        })
        .collect()
}

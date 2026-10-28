use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use rayon::prelude::*;

use super::dsp::{PowerSpectrum, percentile};

const FFT_SIZE: usize = 4096;
const BASS_FFT_SIZE: usize = 8192;
const BASS_BLEND_START: f32 = 150.0;
const BASS_BLEND_END: f32 = 400.0;
const MIN_FREQUENCY: f32 = 24.0;
const MAX_FREQUENCY: f32 = 15_000.0;
const TILT_DB_PER_OCTAVE: f32 = 3.0;
const TILT_PIVOT_FREQUENCY: f32 = 1_000.0;
const DYNAMIC_RANGE_DB: f32 = 45.0;
const REFERENCE_PERCENTILE: f32 = 0.995;
const ATTACK: f32 = 0.7;
const RELEASE: f32 = 0.18;

pub fn decode_mono(path: &Path, sample_rate: u32) -> Result<Vec<f32>> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", &sample_rate.to_string(), "-f", "f32le", "pipe:1"])
        .output()
        .context("Failed to run ffmpeg")?;

    if !output.status.success() {
        bail!(
            "Could not decode the audio file: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let samples: Vec<f32> = output
        .stdout
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect();
    ensure!(!samples.is_empty(), "The audio file contains no samples");
    Ok(samples)
}

pub struct Spectrum {
    bar_count: usize,
    levels: Vec<f32>,
}

impl Spectrum {
    pub fn analyze(samples: &[f32], sample_rate: u32, fps: u32, bar_count: usize) -> Self {
        let samples_per_frame = f64::from(sample_rate) / f64::from(fps);
        let frame_count = (samples.len() as f64 / samples_per_frame).ceil().max(1.0) as usize;
        let bands = Band::log_spaced(bar_count);
        let sample_rate = sample_rate as f32;
        let bass_analyzer = PowerSpectrum::new(BASS_FFT_SIZE);
        let analyzer = PowerSpectrum::new(FFT_SIZE);

        let mut levels = vec![0.0; frame_count * bar_count];
        levels
            .par_chunks_mut(bar_count)
            .enumerate()
            .for_each_init(
                || (bass_analyzer.clone(), analyzer.clone()),
                |(bass_worker, worker), (frame, bars)| {
                    let center = ((frame as f64 + 0.5) * samples_per_frame) as isize;
                    let bass = bass_worker.compute(samples, center);
                    let full = worker.compute(samples, center);
                    for (level, band) in bars.iter_mut().zip(&bands) {
                        *level = band.decibels(bass, full, sample_rate);
                    }
                },
            );

        normalize(&mut levels);
        smooth_across_bars(&mut levels, bar_count);
        smooth_over_time(&mut levels, bar_count);
        Self { bar_count, levels }
    }

    pub fn frame_count(&self) -> usize {
        self.levels.len() / self.bar_count
    }

    pub fn frame(&self, index: usize) -> &[f32] {
        let start = index * self.bar_count;
        &self.levels[start..start + self.bar_count]
    }
}

struct Band {
    low_hz: f32,
    high_hz: f32,
    bass_weight: f32,
    tilt_db: f32,
}

impl Band {
    fn log_spaced(count: usize) -> Vec<Self> {
        let ratio = (MAX_FREQUENCY / MIN_FREQUENCY).powf(1.0 / count as f32);
        let blend_octaves = (BASS_BLEND_END / BASS_BLEND_START).log2();
        (0..count)
            .map(|index| {
                let low = MIN_FREQUENCY * ratio.powi(index as i32);
                let high = low * ratio;
                let center = (low * high).sqrt();
                let blend = (center / BASS_BLEND_START).log2() / blend_octaves;
                Self {
                    low_hz: low,
                    high_hz: high,
                    bass_weight: 1.0 - blend.clamp(0.0, 1.0),
                    tilt_db: TILT_DB_PER_OCTAVE * (center / TILT_PIVOT_FREQUENCY).log2(),
                }
            })
            .collect()
    }

    fn decibels(&self, bass: &[f32], full: &[f32], sample_rate: f32) -> f32 {
        let bass_db = self.measure(bass, BASS_FFT_SIZE, sample_rate);
        let full_db = self.measure(full, FFT_SIZE, sample_rate);
        bass_db * self.bass_weight + full_db * (1.0 - self.bass_weight) + self.tilt_db
    }

    fn measure(&self, power: &[f32], fft_size: usize, sample_rate: f32) -> f32 {
        let bin_hz = sample_rate / fft_size as f32;
        let low_bin = self.low_hz / bin_hz;
        let high_bin = self.high_hz / bin_hz;
        let first = low_bin.ceil() as usize;
        let last = (high_bin.floor() as usize).min(power.len() - 1);
        let mean_power = if last >= first {
            power[first..=last].iter().sum::<f32>() / (last - first + 1) as f32
        } else {
            interpolate(power, f32::midpoint(low_bin, high_bin))
        };
        10.0 * (mean_power / fft_size as f32 + 1e-12).log10()
    }
}

fn interpolate(values: &[f32], position: f32) -> f32 {
    let index = (position.floor() as usize).min(values.len() - 2);
    let fraction = (position - index as f32).clamp(0.0, 1.0);
    values[index] * (1.0 - fraction) + values[index + 1] * fraction
}

fn normalize(levels: &mut [f32]) {
    let Some(ceiling) = percentile(levels, REFERENCE_PERCENTILE) else {
        return;
    };
    let floor = ceiling - DYNAMIC_RANGE_DB;
    for level in levels.iter_mut() {
        *level = ((*level - floor) / DYNAMIC_RANGE_DB).clamp(0.0, 1.0);
    }
}

fn smooth_across_bars(levels: &mut [f32], bar_count: usize) {
    let mut original = vec![0.0; bar_count];
    for frame in levels.chunks_mut(bar_count) {
        original.copy_from_slice(frame);
        for (index, level) in frame.iter_mut().enumerate() {
            let left = original[index.saturating_sub(1)];
            let right = original[(index + 1).min(bar_count - 1)];
            *level = 0.25 * left + 0.5 * original[index] + 0.25 * right;
        }
    }
}

fn smooth_over_time(levels: &mut [f32], bar_count: usize) {
    let mut previous = vec![0.0; bar_count];
    for frame in levels.chunks_mut(bar_count) {
        for (level, last) in frame.iter_mut().zip(previous.iter_mut()) {
            let rate = if *level > *last { ATTACK } else { RELEASE };
            *last += (*level - *last) * rate;
            *level = *last;
        }
    }
}

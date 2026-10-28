use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

#[derive(Clone)]
pub struct PowerSpectrum {
    fft: Arc<dyn Fft<f32>>,
    window: Arc<[f32]>,
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    power: Vec<f32>,
}

impl PowerSpectrum {
    pub fn new(size: usize) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(size);
        let scratch_len = fft.get_inplace_scratch_len();
        Self {
            fft,
            window: hann_window(size).into(),
            buffer: vec![Complex::default(); size],
            scratch: vec![Complex::default(); scratch_len],
            power: vec![0.0; size / 2 + 1],
        }
    }

    pub fn compute(&mut self, samples: &[f32], center: isize) -> &[f32] {
        let start = center - (self.buffer.len() / 2) as isize;
        for (offset, (value, weight)) in self.buffer.iter_mut().zip(self.window.iter()).enumerate() {
            let sample = usize::try_from(start + offset as isize)
                .ok()
                .and_then(|index| samples.get(index))
                .copied()
                .unwrap_or(0.0);
            *value = Complex::new(sample * weight, 0.0);
        }
        self.fft.process_with_scratch(&mut self.buffer, &mut self.scratch);
        for (power, bin) in self.power.iter_mut().zip(&self.buffer) {
            *power = bin.norm_sqr();
        }
        &self.power
    }
}

pub fn percentile(values: &[f32], fraction: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    let index = ((sorted.len() - 1) as f32 * fraction.clamp(0.0, 1.0)).round() as usize;
    let (_, value, _) = sorted.select_nth_unstable_by(index, f32::total_cmp);
    Some(*value)
}

fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|index| {
            let phase = std::f32::consts::TAU * index as f32 / (size - 1) as f32;
            0.5 - 0.5 * phase.cos()
        })
        .collect()
}

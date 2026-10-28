use tiny_skia::Pixmap;

use super::math::{hash, lerp, random_unit, smoothstep};
use super::palette::{Palette, Rgb};
use super::timeline::FrameInput;
use super::{HEIGHT, WIDTH};

const FIELD_DOWNSCALE: usize = 4;
const FIELD_WIDTH: usize = WIDTH as usize / FIELD_DOWNSCALE;
const FIELD_HEIGHT: usize = HEIGHT as usize / FIELD_DOWNSCALE;

const TEXTURE_SIZE: usize = 512;
const NOISE_OCTAVES: u32 = 5;
const NOISE_BASE_PERIOD: usize = 4;
const NOISE_SEED: u32 = 0x5eed;

const DENSITY_SCALE: f32 = 0.9;
const WARP_SCALE: f32 = 0.45;
const WARP_STRENGTH: f32 = 70.0;
const WARP_DRIFT: f32 = 5.0;
const RISE_SPEED: f32 = 12.0;
const TINT_SCALE: f32 = 0.25;
const DENSITY_LOW: f32 = 0.47;
const DENSITY_HIGH: f32 = 0.9;
const TOP_DENSITY: f32 = 0.3;

const BASE_INTENSITY: f32 = 0.12;
const ENERGY_INTENSITY: f32 = 0.3;
const BEAT_INTENSITY: f32 = 0.35;
const COLOR_BRIGHTNESS: f32 = 0.85;

type Color3 = [f32; 3];

#[derive(Clone, Copy)]
struct Tap {
    first: usize,
    second: usize,
    weight: f32,
}

pub struct Smoke {
    texture: NoiseTexture,
    primary: Color3,
    accent: Color3,
    row_density: Vec<f32>,
    rows: Vec<Tap>,
    columns: Vec<Tap>,
}

impl Smoke {
    pub fn new(palette: &Palette) -> Self {
        let row_density = (0..FIELD_HEIGHT)
            .map(|row| {
                let depth = row as f32 / (FIELD_HEIGHT - 1) as f32;
                lerp(TOP_DENSITY, 1.0, smoothstep(0.0, 1.0, depth))
            })
            .collect();

        Self {
            texture: NoiseTexture::generate(),
            primary: channels(palette.primary),
            accent: channels(palette.accent),
            row_density,
            rows: taps(HEIGHT as usize, FIELD_HEIGHT),
            columns: taps(WIDTH as usize, FIELD_WIDTH),
        }
    }

    pub fn render_onto(&self, frame: &mut Pixmap, input: &FrameInput) {
        let field = self.field(input);
        self.screen(frame, &field);
    }

    fn field(&self, input: &FrameInput) -> Vec<Color3> {
        let intensity = BASE_INTENSITY + ENERGY_INTENSITY * input.energy + BEAT_INTENSITY * input.beat;
        let rise = input.travel * RISE_SPEED;
        let drift = input.travel * WARP_DRIFT;
        let mut field = vec![[0.0; 3]; FIELD_WIDTH * FIELD_HEIGHT];

        for (y, (row, density)) in field.chunks_exact_mut(FIELD_WIDTH).zip(&self.row_density).enumerate() {
            let strength = intensity * density;
            let v = y as f32 * DENSITY_SCALE;
            for (x, cell) in row.iter_mut().enumerate() {
                let u = x as f32 * DENSITY_SCALE;
                let warp_x = self.texture.sample(u * WARP_SCALE + drift, v * WARP_SCALE) - 0.5;
                let warp_y = self.texture.sample(u * WARP_SCALE + 211.0, v * WARP_SCALE - drift + 97.0) - 0.5;
                let density = self
                    .texture
                    .sample(u + warp_x * WARP_STRENGTH, v + rise + warp_y * WARP_STRENGTH);
                let alpha = smoothstep(DENSITY_LOW, DENSITY_HIGH, density) * strength;
                let tint = self.texture.sample(u * TINT_SCALE - drift, v * TINT_SCALE + 331.0);
                *cell = mix(self.primary, self.accent, tint).map(|channel| channel * alpha);
            }
        }
        field
    }

    fn screen(&self, frame: &mut Pixmap, field: &[Color3]) {
        let mut row = vec![[0.0; 3]; FIELD_WIDTH];
        let stride = WIDTH as usize * 4;

        for (line, tap) in frame.data_mut().chunks_exact_mut(stride).zip(&self.rows) {
            let upper = &field[tap.first * FIELD_WIDTH..][..FIELD_WIDTH];
            let lower = &field[tap.second * FIELD_WIDTH..][..FIELD_WIDTH];
            for ((cell, top), bottom) in row.iter_mut().zip(upper).zip(lower) {
                *cell = mix(*top, *bottom, tap.weight);
            }

            for (pixel, column) in line.chunks_exact_mut(4).zip(&self.columns) {
                let smoke = mix(row[column.first], row[column.second], column.weight);
                for (channel, amount) in pixel[..3].iter_mut().zip(smoke) {
                    let base = f32::from(*channel);
                    *channel = (base + amount * (255.0 - base) + 0.5) as u8;
                }
            }
        }
    }
}

struct NoiseTexture {
    values: Vec<f32>,
}

impl NoiseTexture {
    fn generate() -> Self {
        let mut values = vec![0.0_f32; TEXTURE_SIZE * TEXTURE_SIZE];
        let mut amplitude = 1.0;
        for octave in 0..NOISE_OCTAVES {
            let period = NOISE_BASE_PERIOD << octave;
            let texels_per_cell = (TEXTURE_SIZE / period) as f32;
            let seed = hash(NOISE_SEED.wrapping_add(octave));
            for (index, value) in values.iter_mut().enumerate() {
                let x = (index % TEXTURE_SIZE) as f32 / texels_per_cell;
                let y = (index / TEXTURE_SIZE) as f32 / texels_per_cell;
                *value += amplitude * value_noise(x, y, period, seed);
            }
            amplitude *= 0.5;
        }

        let min = values.iter().copied().fold(f32::INFINITY, f32::min);
        let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let span = (max - min).max(f32::EPSILON);
        for value in &mut values {
            *value = (*value - min) / span;
        }
        Self { values }
    }

    fn sample(&self, x: f32, y: f32) -> f32 {
        let x = x.rem_euclid(TEXTURE_SIZE as f32);
        let y = y.rem_euclid(TEXTURE_SIZE as f32);
        let x0 = x as usize % TEXTURE_SIZE;
        let y0 = y as usize % TEXTURE_SIZE;
        let x1 = (x0 + 1) % TEXTURE_SIZE;
        let y1 = (y0 + 1) % TEXTURE_SIZE;
        let fx = x.fract();
        let fy = y.fract();

        let value = |column: usize, row: usize| self.values[row * TEXTURE_SIZE + column];
        let top = lerp(value(x0, y0), value(x1, y0), fx);
        let bottom = lerp(value(x0, y1), value(x1, y1), fx);
        lerp(top, bottom, fy)
    }
}

fn value_noise(x: f32, y: f32, period: usize, seed: u32) -> f32 {
    let x0 = x.floor() as usize % period;
    let y0 = y.floor() as usize % period;
    let x1 = (x0 + 1) % period;
    let y1 = (y0 + 1) % period;
    let fx = quintic(x.fract());
    let fy = quintic(y.fract());

    let lattice = |column: usize, row: usize| random_unit(hash(column as u32 ^ seed) ^ row as u32);
    let top = lerp(lattice(x0, y0), lattice(x1, y0), fx);
    let bottom = lerp(lattice(x0, y1), lattice(x1, y1), fx);
    lerp(top, bottom, fy)
}

fn quintic(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn taps(output: usize, input: usize) -> Vec<Tap> {
    let ratio = input as f32 / output as f32;
    (0..output)
        .map(|index| {
            let position = ((index as f32 + 0.5) * ratio - 0.5).clamp(0.0, (input - 1) as f32);
            let first = position as usize;
            Tap {
                first,
                second: (first + 1).min(input - 1),
                weight: position - first as f32,
            }
        })
        .collect()
}

fn channels(color: Rgb) -> Color3 {
    [color.r, color.g, color.b].map(|channel| channel * COLOR_BRIGHTNESS)
}

fn mix(from: Color3, to: Color3, amount: f32) -> Color3 {
    [0, 1, 2].map(|index| lerp(from[index], to[index], amount))
}

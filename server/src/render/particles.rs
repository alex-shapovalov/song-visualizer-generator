use std::f32::consts::TAU;

use anyhow::{Context, Result};
use tiny_skia::{
    BlendMode, FilterQuality, GradientStop, Paint, Pixmap, PixmapPaint, Point, RadialGradient, Rect, SpreadMode,
    Transform,
};

use super::math::{lerp, random_unit, smoothstep};
use super::palette::{Palette, Rgb};
use super::timeline::FrameInput;
use super::{HEIGHT, SCALE, WIDTH};

const COUNT: u32 = 110;
const SEED: u32 = 0x00c0_ffee;
const SPRITE_SIZE: u32 = 64;
const MIN_RADIUS: f32 = 1.2 * SCALE;
const MAX_RADIUS: f32 = 4.0 * SCALE;
const GLOW_RATIO: f32 = 5.0;
const MIN_SPEED: f32 = 0.5;
const MAX_SPEED: f32 = 1.6;
const RISE_SPEED: f32 = 22.0 * SCALE;
const MAX_SWAY: f32 = 28.0 * SCALE;
const BASE_OPACITY: f32 = 0.35;
const ENERGY_OPACITY: f32 = 0.45;
const BEAT_OPACITY: f32 = 0.35;
const TOP_FADE: f32 = 0.25;
const BOTTOM_FADE: f32 = 0.1;
const MIN_VISIBLE_OPACITY: f32 = 0.01;

struct Particle {
    x: f32,
    offset: f32,
    radius: f32,
    speed: f32,
    sway: f32,
    sway_rate: f32,
    phase: f32,
    twinkle_rate: f32,
    brightness: f32,
    sprite: usize,
}

pub struct Particles {
    particles: Vec<Particle>,
    sprites: [Pixmap; 2],
}

impl Particles {
    pub fn new(palette: &Palette) -> Result<Self> {
        let particles = (0..COUNT).map(Particle::random).collect();
        let sprites = [
            glow_sprite(palette.primary.mix(Rgb::WHITE, 0.35))?,
            glow_sprite(palette.accent.mix(Rgb::WHITE, 0.35))?,
        ];
        Ok(Self { particles, sprites })
    }

    pub fn draw(&self, frame: &mut Pixmap, input: &FrameInput) {
        let level = (BASE_OPACITY + ENERGY_OPACITY * input.energy + BEAT_OPACITY * input.beat).min(1.0);
        let margin = MAX_RADIUS * GLOW_RATIO;
        let span = HEIGHT as f32 + margin * 2.0;

        for particle in &self.particles {
            let progress = (particle.offset * span + input.travel * RISE_SPEED * particle.speed) % span;
            let y = HEIGHT as f32 + margin - progress;
            let x = particle.x + particle.sway * (input.seconds * particle.sway_rate + particle.phase).sin();

            let height = y / HEIGHT as f32;
            let fade = smoothstep(0.0, TOP_FADE, height) * smoothstep(1.0, 1.0 - BOTTOM_FADE, height);
            let twinkle = 0.75 + 0.25 * (input.seconds * particle.twinkle_rate + particle.phase).sin();
            let opacity = (particle.brightness * level * fade * twinkle).clamp(0.0, 1.0);
            if opacity < MIN_VISIBLE_OPACITY {
                continue;
            }

            let glow_radius = particle.radius * GLOW_RATIO;
            let scale = glow_radius * 2.0 / SPRITE_SIZE as f32;
            let transform = Transform::from_row(scale, 0.0, 0.0, scale, x - glow_radius, y - glow_radius);
            let paint = PixmapPaint {
                opacity,
                blend_mode: BlendMode::Screen,
                quality: FilterQuality::Bilinear,
            };
            frame.draw_pixmap(0, 0, self.sprites[particle.sprite].as_ref(), &paint, transform, None);
        }
    }
}

impl Particle {
    fn random(index: u32) -> Self {
        let random = |slot: u32| random_unit(SEED.wrapping_add(index * 16 + slot));
        let depth = random(2);
        Self {
            x: random(0) * WIDTH as f32,
            offset: random(1),
            radius: lerp(MIN_RADIUS, MAX_RADIUS, depth * depth),
            speed: lerp(MIN_SPEED, MAX_SPEED, depth),
            sway: random(3) * MAX_SWAY,
            sway_rate: lerp(0.15, 0.5, random(4)) * TAU,
            phase: random(5) * TAU,
            twinkle_rate: lerp(0.8, 2.5, random(6)) * TAU,
            brightness: lerp(0.5, 1.0, random(7)),
            sprite: usize::from(random(8) >= 0.5),
        }
    }
}

fn glow_sprite(color: Rgb) -> Result<Pixmap> {
    let mut sprite = Pixmap::new(SPRITE_SIZE, SPRITE_SIZE).context("Could not allocate a particle sprite")?;
    let size = SPRITE_SIZE as f32;
    let center = Point::from_xy(size / 2.0, size / 2.0);
    let shader = RadialGradient::new(
        center,
        0.0,
        center,
        size / 2.0,
        vec![
            GradientStop::new(0.0, Rgb::WHITE.to_color(1.0)),
            GradientStop::new(1.0 / GLOW_RATIO, color.to_color(0.85)),
            GradientStop::new(0.45, color.to_color(0.22)),
            GradientStop::new(1.0, color.to_color(0.0)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .context("Invalid particle gradient")?;

    let bounds = Rect::from_xywh(0.0, 0.0, size, size).context("Invalid particle sprite size")?;
    let paint = Paint {
        shader,
        ..Paint::default()
    };
    sprite.fill_rect(bounds, &paint, Transform::identity(), None);
    Ok(sprite)
}

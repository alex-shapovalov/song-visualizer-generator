use anyhow::{Context, Result};
use cosmic_text::Weight;
use image::RgbaImage;
use image::imageops::{self, FilterType};
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, LinearGradient, Mask, MaskType, Paint, PathBuilder, Pattern,
    Pixmap, PixmapPaint, Point, RadialGradient, Rect, Shader, SpreadMode, Transform,
};

use super::graphics::{self, Shadow};
use super::palette::{Palette, Rgb};
use super::particles::Particles;
use super::smoke::Smoke;
use super::text::{TextRenderer, TextStyle};
use super::timeline::FrameInput;
use super::{HEIGHT, WIDTH};

pub const BAR_COUNT: usize = 80;

const PULSE_AMPLITUDE: f32 = 0.05;
const ART_LAYER_SCALE: f32 = 1.0 + PULSE_AMPLITUDE;

const CONTENT_TOP: f32 = 70.0;
const CONTENT_BOTTOM: f32 = 790.0;
const ART_SIZE: f32 = 500.0;
const ART_CORNER_RADIUS: f32 = 22.0;
const ART_TITLE_GAP: f32 = 34.0;
const TITLE_ARTIST_GAP: i32 = 0;
const MAX_TEXT_WIDTH: f32 = 1400.0;
const TITLE_SIZE: f32 = 60.0;
const TITLE_MIN_SIZE: f32 = 36.0;
const ARTIST_SIZE: f32 = 36.0;
const ARTIST_MIN_SIZE: f32 = 26.0;

const GLOW_SIGMA: f32 = 60.0;
const GLOW_OPACITY: f32 = 0.45;
const SHADOW_SIGMA: f32 = 22.0;
const SHADOW_OPACITY: f32 = 0.55;
const SHADOW_OFFSET: f32 = 16.0;
const TEXT_SHADOW_SIGMA: f32 = 8.0;
const TEXT_SHADOW_OPACITY: f32 = 0.5;
const TEXT_SHADOW_OFFSET: i32 = 3;

const SPECTRUM_MARGIN: f32 = 160.0;
const SPECTRUM_BASELINE: f32 = 985.0;
const BAR_MAX_HEIGHT: f32 = 170.0;
const BAR_FILL_RATIO: f32 = 0.6;
const REFLECTION_SCALE: f32 = 0.35;
const REFLECTION_GAP: f32 = 6.0;
const REFLECTION_OPACITY: f32 = 0.28;

const BACKGROUND_SAMPLE_WIDTH: u32 = 480;
const BACKGROUND_BLUR_SIGMA: f32 = 10.0;
const BACKGROUND_DIM: f32 = 0.45;
const VIGNETTE_OPACITY: f32 = 0.6;
const BOTTOM_FADE_START: f32 = 0.55;
const BOTTOM_FADE_OPACITY: f32 = 0.55;

pub struct Scene {
    background: Pixmap,
    smoke: Smoke,
    particles: Particles,
    art: Pixmap,
    art_anchor: Point,
    art_center: Point,
    text: Composite,
    bar_paint: Paint<'static>,
    reflection_mask: Mask,
}

struct Placement<'a> {
    pixmap: &'a Pixmap,
    x: i32,
    y: i32,
}

struct Composite {
    pixmap: Pixmap,
    left: i32,
    top: i32,
}

struct TextBlock {
    layer: Composite,
    height: i32,
}

impl Scene {
    pub fn build(
        art: &RgbaImage,
        palette: &Palette,
        title: &str,
        artist: &str,
        text: &mut TextRenderer,
    ) -> Result<Self> {
        let (art_layer, art_anchor) = build_art_layer(art, palette)?;
        let text_block = build_text_block(palette, title, artist, text)?;

        let content_height = ART_SIZE + ART_TITLE_GAP + text_block.height as f32;
        let content_top = f32::midpoint(CONTENT_TOP, CONTENT_BOTTOM) - content_height / 2.0;
        let text_top = (content_top + ART_SIZE + ART_TITLE_GAP).round() as i32;

        Ok(Self {
            background: build_background(art, palette)?,
            smoke: Smoke::new(palette),
            particles: Particles::new(palette)?,
            art: art_layer,
            art_anchor,
            art_center: Point::from_xy(WIDTH as f32 / 2.0, content_top + ART_SIZE / 2.0),
            text: Composite {
                left: text_block.layer.left + (WIDTH / 2) as i32,
                top: text_block.layer.top + text_top,
                pixmap: text_block.layer.pixmap,
            },
            bar_paint: bar_paint(palette)?,
            reflection_mask: reflection_mask()?,
        })
    }

    pub fn render_frame(&self, input: &FrameInput) -> Pixmap {
        let mut frame = self.background.clone();
        self.smoke.render_onto(&mut frame, input);
        self.particles.draw(&mut frame, input);

        let scale = (1.0 + PULSE_AMPLITUDE * input.beat) / ART_LAYER_SCALE;
        let transform = Transform::from_row(
            scale,
            0.0,
            0.0,
            scale,
            self.art_center.x - self.art_anchor.x * scale,
            self.art_center.y - self.art_anchor.y * scale,
        );
        let paint = PixmapPaint {
            quality: FilterQuality::Bicubic,
            ..PixmapPaint::default()
        };
        frame.draw_pixmap(0, 0, self.art.as_ref(), &paint, transform, None);
        graphics::draw_at(&mut frame, &self.text.pixmap, self.text.left, self.text.top);

        self.draw_spectrum(&mut frame, input.levels);
        if input.brightness < 1.0 {
            graphics::dim(&mut frame, input.brightness);
        }
        frame
    }

    fn draw_spectrum(&self, frame: &mut Pixmap, levels: &[f32]) {
        let slot = (WIDTH as f32 - SPECTRUM_MARGIN * 2.0) / levels.len() as f32;
        let bar_width = slot * BAR_FILL_RATIO;
        let mut builder = PathBuilder::new();
        for (index, level) in levels.iter().enumerate() {
            let height = bar_width + level * (BAR_MAX_HEIGHT - bar_width);
            let x = SPECTRUM_MARGIN + index as f32 * slot + (slot - bar_width) / 2.0;
            graphics::push_rounded_rect(
                &mut builder,
                x,
                SPECTRUM_BASELINE - height,
                bar_width,
                height,
                bar_width / 2.0,
            );
        }
        let Some(bars) = builder.finish() else {
            return;
        };

        frame.fill_path(&bars, &self.bar_paint, FillRule::Winding, Transform::identity(), None);

        let reflection = Transform::from_row(
            1.0,
            0.0,
            0.0,
            -REFLECTION_SCALE,
            0.0,
            SPECTRUM_BASELINE * (1.0 + REFLECTION_SCALE) + REFLECTION_GAP,
        );
        frame.fill_path(&bars, &self.bar_paint, FillRule::Winding, reflection, Some(&self.reflection_mask));
    }
}

fn build_art_layer(art: &RgbaImage, palette: &Palette) -> Result<(Pixmap, Point)> {
    let scale = ART_LAYER_SCALE;
    let art_layer = rounded_art(art, (ART_SIZE * scale).round() as u32, ART_CORNER_RADIUS * scale)?;
    let glow = graphics::soft_shadow(&art_layer, GLOW_SIGMA * scale, palette.primary.to_rgb8(), GLOW_OPACITY)?;
    let shadow = graphics::soft_shadow(&art_layer, SHADOW_SIGMA * scale, [0, 0, 0], SHADOW_OPACITY)?;
    let shadow_offset = (SHADOW_OFFSET * scale).round() as i32;

    let layer = compose(&[
        behind(&glow, 0, 0, 0),
        behind(&shadow, 0, 0, shadow_offset),
        Placement {
            pixmap: &art_layer,
            x: 0,
            y: 0,
        },
    ])?;

    let half_size = art_layer.width() as f32 / 2.0;
    let anchor = Point::from_xy(half_size - layer.left as f32, half_size - layer.top as f32);
    Ok((layer.pixmap, anchor))
}

fn build_text_block(palette: &Palette, title: &str, artist: &str, text: &mut TextRenderer) -> Result<TextBlock> {
    let title_color = Rgb::WHITE.mix(palette.primary, 0.06).to_rgba8(1.0);
    let artist_color = Rgb::WHITE.mix(palette.primary, 0.4).to_rgba8(0.92);
    let title_layer = text.render(
        title,
        &TextStyle {
            size: TITLE_SIZE,
            min_size: TITLE_MIN_SIZE,
            weight: Weight::BOLD,
            color: text_color(title_color),
        },
        MAX_TEXT_WIDTH,
    )?;
    let artist_layer = text.render(
        artist,
        &TextStyle {
            size: ARTIST_SIZE,
            min_size: ARTIST_MIN_SIZE,
            weight: Weight::MEDIUM,
            color: text_color(artist_color),
        },
        MAX_TEXT_WIDTH,
    )?;
    let title_shadow = graphics::soft_shadow(&title_layer, TEXT_SHADOW_SIGMA, [0, 0, 0], TEXT_SHADOW_OPACITY)?;
    let artist_shadow = graphics::soft_shadow(&artist_layer, TEXT_SHADOW_SIGMA, [0, 0, 0], TEXT_SHADOW_OPACITY)?;

    let title_x = -(title_layer.width() as i32) / 2;
    let artist_x = -(artist_layer.width() as i32) / 2;
    let artist_y = title_layer.height() as i32 + TITLE_ARTIST_GAP;

    let layer = compose(&[
        behind(&title_shadow, title_x, 0, TEXT_SHADOW_OFFSET),
        Placement {
            pixmap: &title_layer,
            x: title_x,
            y: 0,
        },
        behind(&artist_shadow, artist_x, artist_y, TEXT_SHADOW_OFFSET),
        Placement {
            pixmap: &artist_layer,
            x: artist_x,
            y: artist_y,
        },
    ])?;

    Ok(TextBlock {
        layer,
        height: artist_y + artist_layer.height() as i32,
    })
}

fn compose(placements: &[Placement]) -> Result<Composite> {
    let left = placements.iter().map(|p| p.x).min().unwrap_or_default();
    let top = placements.iter().map(|p| p.y).min().unwrap_or_default();
    let right = placements.iter().map(|p| p.x + p.pixmap.width() as i32).max().unwrap_or_default();
    let bottom = placements.iter().map(|p| p.y + p.pixmap.height() as i32).max().unwrap_or_default();

    let mut pixmap = Pixmap::new((right - left) as u32, (bottom - top) as u32).context("Could not allocate a layer")?;
    for placement in placements {
        graphics::draw_at(&mut pixmap, placement.pixmap, placement.x - left, placement.y - top);
    }
    Ok(Composite { pixmap, left, top })
}

fn behind(shadow: &Shadow, x: i32, y: i32, offset_y: i32) -> Placement<'_> {
    Placement {
        pixmap: &shadow.pixmap,
        x: x - shadow.padding,
        y: y - shadow.padding + offset_y,
    }
}

fn text_color([r, g, b, a]: [u8; 4]) -> cosmic_text::Color {
    cosmic_text::Color::rgba(r, g, b, a)
}

fn rounded_art(art: &RgbaImage, size: u32, radius: f32) -> Result<Pixmap> {
    let square = graphics::cover_crop(art, 1.0);
    let resized = imageops::resize(&square, size, size, FilterType::Lanczos3);
    let source = graphics::pixmap_from_image(&resized)?;

    let mut layer = Pixmap::new(size, size).context("Could not allocate the album art layer")?;
    let path = graphics::rounded_rect(0.0, 0.0, size as f32, size as f32, radius).context("Invalid album art size")?;
    let paint = Paint {
        shader: Pattern::new(
            source.as_ref(),
            SpreadMode::Pad,
            FilterQuality::Nearest,
            1.0,
            Transform::identity(),
        ),
        ..Paint::default()
    };
    layer.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    Ok(layer)
}

fn build_background(art: &RgbaImage, palette: &Palette) -> Result<Pixmap> {
    let aspect_ratio = WIDTH as f32 / HEIGHT as f32;
    let sample_height = (BACKGROUND_SAMPLE_WIDTH as f32 / aspect_ratio).round() as u32;
    let cropped = graphics::cover_crop(art, aspect_ratio);
    let sample = imageops::resize(&cropped, BACKGROUND_SAMPLE_WIDTH, sample_height, FilterType::Triangle);
    let blurred = imageops::fast_blur(&sample, BACKGROUND_BLUR_SIGMA);
    let upscaled = imageops::resize(&blurred, WIDTH, HEIGHT, FilterType::Triangle);
    let art_layer = graphics::pixmap_from_image(&upscaled)?;

    let mut background = Pixmap::new(WIDTH, HEIGHT).context("Could not allocate the background")?;
    let shade = palette.shade;
    background.fill(shade.to_color(1.0));
    graphics::draw_at(&mut background, &art_layer, 0, 0);

    let full = Rect::from_xywh(0.0, 0.0, WIDTH as f32, HEIGHT as f32).context("Invalid frame size")?;
    let center = Point::from_xy(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0);
    let corner_distance = center.x.hypot(center.y);

    let overlays = [
        Shader::SolidColor(shade.to_color(BACKGROUND_DIM)),
        RadialGradient::new(
            center,
            0.0,
            center,
            corner_distance,
            vec![
                GradientStop::new(0.0, shade.to_color(0.0)),
                GradientStop::new(1.0, shade.to_color(VIGNETTE_OPACITY)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .context("Invalid vignette gradient")?,
        LinearGradient::new(
            Point::from_xy(0.0, HEIGHT as f32 * BOTTOM_FADE_START),
            Point::from_xy(0.0, HEIGHT as f32),
            vec![
                GradientStop::new(0.0, shade.to_color(0.0)),
                GradientStop::new(1.0, shade.to_color(BOTTOM_FADE_OPACITY)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .context("Invalid bottom fade gradient")?,
    ];
    for shader in overlays {
        let paint = Paint {
            shader,
            ..Paint::default()
        };
        background.fill_rect(full, &paint, Transform::identity(), None);
    }

    graphics::dither(&mut background);
    Ok(background)
}

fn bar_paint(palette: &Palette) -> Result<Paint<'static>> {
    let shader = LinearGradient::new(
        Point::from_xy(SPECTRUM_MARGIN, 0.0),
        Point::from_xy(WIDTH as f32 - SPECTRUM_MARGIN, 0.0),
        vec![
            GradientStop::new(0.0, palette.primary.to_color(1.0)),
            GradientStop::new(1.0, palette.accent.to_color(1.0)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .context("Invalid spectrum gradient")?;
    Ok(Paint {
        shader,
        ..Paint::default()
    })
}

fn reflection_mask() -> Result<Mask> {
    let top = SPECTRUM_BASELINE + REFLECTION_GAP;
    let bottom = top + BAR_MAX_HEIGHT * REFLECTION_SCALE;
    let fade = LinearGradient::new(
        Point::from_xy(0.0, top),
        Point::from_xy(0.0, bottom),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, (REFLECTION_OPACITY * 255.0).round() as u8)),
            GradientStop::new(1.0, Color::TRANSPARENT),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .context("Invalid reflection gradient")?;

    let mut pixmap = Pixmap::new(WIDTH, HEIGHT).context("Could not allocate the reflection mask")?;
    let region = Rect::from_ltrb(0.0, top, WIDTH as f32, HEIGHT as f32).context("Invalid reflection region")?;
    let paint = Paint {
        shader: fade,
        ..Paint::default()
    };
    pixmap.fill_rect(region, &paint, Transform::identity(), None);
    Ok(Mask::from_pixmap(pixmap.as_ref(), MaskType::Alpha))
}

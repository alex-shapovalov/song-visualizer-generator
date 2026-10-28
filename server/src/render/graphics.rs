use anyhow::{Context, Result};
use image::{Rgba, RgbaImage, imageops};
use tiny_skia::{IntSize, Path, PathBuilder, Pixmap, PixmapPaint, Transform};

use super::math::hash;

const KAPPA: f32 = 0.552_284_8;

pub struct Shadow {
    pub pixmap: Pixmap,
    pub padding: i32,
}

pub fn push_rounded_rect(builder: &mut PathBuilder, x: f32, y: f32, width: f32, height: f32, radius: f32) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    let handle = radius * (1.0 - KAPPA);
    let right = x + width;
    let bottom = y + height;

    builder.move_to(x + radius, y);
    builder.line_to(right - radius, y);
    builder.cubic_to(right - handle, y, right, y + handle, right, y + radius);
    builder.line_to(right, bottom - radius);
    builder.cubic_to(right, bottom - handle, right - handle, bottom, right - radius, bottom);
    builder.line_to(x + radius, bottom);
    builder.cubic_to(x + handle, bottom, x, bottom - handle, x, bottom - radius);
    builder.line_to(x, y + radius);
    builder.cubic_to(x, y + handle, x + handle, y, x + radius, y);
    builder.close();
}

pub fn rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Option<Path> {
    let mut builder = PathBuilder::new();
    push_rounded_rect(&mut builder, x, y, width, height, radius);
    builder.finish()
}

pub fn cover_crop(image: &RgbaImage, aspect_ratio: f32) -> RgbaImage {
    let (width, height) = image.dimensions();
    let (crop_width, crop_height) = if width as f32 / height as f32 > aspect_ratio {
        ((height as f32 * aspect_ratio).round() as u32, height)
    } else {
        (width, (width as f32 / aspect_ratio).round() as u32)
    };
    let crop_width = crop_width.clamp(1, width);
    let crop_height = crop_height.clamp(1, height);
    imageops::crop_imm(
        image,
        (width - crop_width) / 2,
        (height - crop_height) / 2,
        crop_width,
        crop_height,
    )
    .to_image()
}

pub fn pixmap_from_image(image: &RgbaImage) -> Result<Pixmap> {
    let mut data = image.as_raw().clone();
    for pixel in data.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let size = IntSize::from_wh(image.width(), image.height()).context("Image has no pixels")?;
    Pixmap::from_vec(data, size).context("Could not convert image to a pixmap")
}

pub fn soft_shadow(source: &Pixmap, sigma: f32, color: [u8; 3], opacity: f32) -> Result<Shadow> {
    let padding = (sigma * 3.0).ceil() as u32;
    let width = source.width() + padding * 2;
    let height = source.height() + padding * 2;

    let mut silhouette = RgbaImage::new(width, height);
    for (index, pixel) in source.pixels().iter().enumerate() {
        let x = index as u32 % source.width() + padding;
        let y = index as u32 / source.width() + padding;
        let alpha = (f32::from(pixel.alpha()) * opacity).round() as u8;
        let tint = color.map(|channel| ((u16::from(channel) * u16::from(alpha) + 127) / 255) as u8);
        silhouette.put_pixel(x, y, Rgba([tint[0], tint[1], tint[2], alpha]));
    }

    let mut blurred = imageops::fast_blur(&silhouette, sigma).into_raw();
    for pixel in blurred.chunks_exact_mut(4) {
        let alpha = pixel[3];
        for channel in &mut pixel[..3] {
            *channel = (*channel).min(alpha);
        }
    }

    let size = IntSize::from_wh(width, height).context("Shadow has no pixels")?;
    let pixmap = Pixmap::from_vec(blurred, size).context("Could not build shadow pixmap")?;
    Ok(Shadow {
        pixmap,
        padding: padding as i32,
    })
}

pub fn draw_at(target: &mut Pixmap, source: &Pixmap, x: i32, y: i32) {
    target.draw_pixmap(x, y, source.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

pub fn dim(pixmap: &mut Pixmap, brightness: f32) {
    let scale = (brightness.clamp(0.0, 1.0) * 256.0).round() as u16;
    for pixel in pixmap.data_mut().chunks_exact_mut(4) {
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * scale) >> 8) as u8;
        }
    }
}

pub fn dither(pixmap: &mut Pixmap) {
    for (index, pixel) in pixmap.data_mut().chunks_exact_mut(4).enumerate() {
        let offset = (hash(index as u32) % 3) as i16 - 1;
        for channel in &mut pixel[..3] {
            *channel = (i16::from(*channel) + offset).clamp(0, 255) as u8;
        }
    }
}

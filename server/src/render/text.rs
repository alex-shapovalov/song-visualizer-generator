use anyhow::{Context, Result};
use cosmic_text::{Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, Wrap};
use tiny_skia::Pixmap;

const INTER_BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Inter-Medium.ttf");
const FONT_FAMILY: &str = "Inter";
const LINE_HEIGHT_RATIO: f32 = 1.2;
const PADDING_RATIO: f32 = 0.15;
const FIT_SAFETY_RATIO: f32 = 0.98;

pub struct TextStyle {
    pub size: f32,
    pub min_size: f32,
    pub weight: Weight,
    pub color: Color,
}

pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

struct Bounds {
    left: f32,
    right: f32,
    bottom: f32,
}

impl TextRenderer {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new();
        let database = font_system.db_mut();
        database.load_font_data(INTER_BOLD.to_vec());
        database.load_font_data(INTER_MEDIUM.to_vec());
        Self {
            font_system,
            swash_cache: SwashCache::new(),
        }
    }

    pub fn render(&mut self, text: &str, style: &TextStyle, max_width: f32) -> Result<Pixmap> {
        let attrs = Attrs::new().family(Family::Name(FONT_FAMILY)).weight(style.weight);
        let natural_width = self.measure(text, &attrs, style.size);
        let size = if natural_width > max_width {
            (style.size * FIT_SAFETY_RATIO * max_width / natural_width).max(style.min_size)
        } else {
            style.size
        };

        let mut buffer = Buffer::new(&mut self.font_system, metrics(size));
        buffer.set_wrap(Wrap::WordOrGlyph);
        buffer.set_size(Some(max_width), None);
        buffer.set_text(text, &attrs, Shaping::Advanced, Some(Align::Center));
        buffer.shape_until_scroll(&mut self.font_system, false);

        let bounds = text_bounds(&buffer).context("The text has no visible characters")?;
        let padding = (size * PADDING_RATIO).ceil();
        let width = (bounds.right - bounds.left + padding * 2.0).ceil() as u32;
        let height = (bounds.bottom + padding * 2.0).ceil() as u32;
        let mut pixmap = Pixmap::new(width, height).context("The text is too large to render")?;

        let offset_x = (padding - bounds.left).round() as i32;
        let offset_y = padding as i32;
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            style.color,
            |x, y, rect_width, rect_height, color| {
                blend_rect(&mut pixmap, x + offset_x, y + offset_y, rect_width, rect_height, color);
            },
        );
        Ok(pixmap)
    }

    fn measure(&mut self, text: &str, attrs: &Attrs, size: f32) -> f32 {
        let mut buffer = Buffer::new(&mut self.font_system, metrics(size));
        buffer.set_wrap(Wrap::None);
        buffer.set_text(text, attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer.layout_runs().map(|run| run.line_w).fold(0.0, f32::max)
    }
}

fn metrics(size: f32) -> Metrics {
    Metrics::new(size, size * LINE_HEIGHT_RATIO)
}

fn text_bounds(buffer: &Buffer) -> Option<Bounds> {
    let mut bounds: Option<Bounds> = None;
    for run in buffer.layout_runs() {
        for glyph in run.glyphs {
            let current = bounds.get_or_insert(Bounds {
                left: f32::INFINITY,
                right: f32::NEG_INFINITY,
                bottom: 0.0,
            });
            current.left = current.left.min(glyph.x);
            current.right = current.right.max(glyph.x + glyph.w);
            current.bottom = current.bottom.max(run.line_top + run.line_height);
        }
    }
    bounds
}

fn blend_rect(pixmap: &mut Pixmap, x: i32, y: i32, width: u32, height: u32, color: Color) {
    let alpha = u32::from(color.a());
    if alpha == 0 {
        return;
    }
    let inverse = 255 - alpha;
    let stride = pixmap.width() as i32;
    let rows = pixmap.height() as i32;
    let source = [color.r(), color.g(), color.b()];
    let data = pixmap.data_mut();

    for row in y.max(0)..(y + height as i32).min(rows) {
        for column in x.max(0)..(x + width as i32).min(stride) {
            let offset = ((row * stride + column) * 4) as usize;
            let pixel = &mut data[offset..offset + 4];
            for (channel, value) in pixel[..3].iter_mut().zip(source) {
                *channel = ((u32::from(value) * alpha + u32::from(*channel) * inverse + 127) / 255) as u8;
            }
            pixel[3] = ((alpha * 255 + u32::from(pixel[3]) * inverse + 127) / 255) as u8;
        }
    }
}

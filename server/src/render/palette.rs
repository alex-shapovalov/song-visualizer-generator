use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};
use tiny_skia::Color;

const SAMPLE_SIZE: u32 = 48;
const CLUSTER_COUNT: usize = 6;
const ITERATIONS: usize = 12;
const ACHROMATIC_CHROMA: f32 = 0.08;
const MIN_ACCENT_HUE_DISTANCE: f32 = 40.0;
const FALLBACK_ACCENT_HUE_SHIFT: f32 = 35.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const WHITE: Self = Self::gray(1.0);

    pub const fn gray(value: f32) -> Self {
        Self {
            r: value,
            g: value,
            b: value,
        }
    }

    fn from_pixel(pixel: &Rgba<u8>) -> Self {
        Self {
            r: f32::from(pixel[0]) / 255.0,
            g: f32::from(pixel[1]) / 255.0,
            b: f32::from(pixel[2]) / 255.0,
        }
    }

    pub fn mix(self, other: Self, amount: f32) -> Self {
        Self {
            r: self.r + (other.r - self.r) * amount,
            g: self.g + (other.g - self.g) * amount,
            b: self.b + (other.b - self.b) * amount,
        }
    }

    pub fn to_rgb8(self) -> [u8; 3] {
        [self.r, self.g, self.b].map(to_byte)
    }

    pub fn to_rgba8(self, alpha: f32) -> [u8; 4] {
        let [r, g, b] = self.to_rgb8();
        [r, g, b, to_byte(alpha)]
    }

    pub fn to_color(self, alpha: f32) -> Color {
        let [r, g, b, a] = self.to_rgba8(alpha);
        Color::from_rgba8(r, g, b, a)
    }

    fn chroma(self) -> f32 {
        self.r.max(self.g).max(self.b) - self.r.min(self.g).min(self.b)
    }

    fn distance_squared(self, other: Self) -> f32 {
        (self.r - other.r).powi(2) + (self.g - other.g).powi(2) + (self.b - other.b).powi(2)
    }

    fn to_hsl(self) -> Hsl {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let lightness = f32::midpoint(max, min);
        let delta = max - min;
        if delta <= f32::EPSILON {
            return Hsl {
                hue: 0.0,
                saturation: 0.0,
                lightness,
            };
        }

        let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
        let sector = if max == self.r {
            ((self.g - self.b) / delta).rem_euclid(6.0)
        } else if max == self.g {
            (self.b - self.r) / delta + 2.0
        } else {
            (self.r - self.g) / delta + 4.0
        };
        Hsl {
            hue: sector * 60.0,
            saturation,
            lightness,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Hsl {
    hue: f32,
    saturation: f32,
    lightness: f32,
}

impl Hsl {
    fn to_rgb(self) -> Rgb {
        let chroma = (1.0 - (2.0 * self.lightness - 1.0).abs()) * self.saturation;
        let sector = self.hue.rem_euclid(360.0) / 60.0;
        let secondary = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
        let (r, g, b) = match sector as u32 {
            0 => (chroma, secondary, 0.0),
            1 => (secondary, chroma, 0.0),
            2 => (0.0, chroma, secondary),
            3 => (0.0, secondary, chroma),
            4 => (secondary, 0.0, chroma),
            _ => (chroma, 0.0, secondary),
        };
        let offset = self.lightness - chroma / 2.0;
        Rgb {
            r: (r + offset).clamp(0.0, 1.0),
            g: (g + offset).clamp(0.0, 1.0),
            b: (b + offset).clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Cluster {
    color: Rgb,
    weight: f32,
}

impl Cluster {
    fn vibrancy(&self) -> f32 {
        self.weight.sqrt() * (self.color.chroma() + 0.02)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub primary: Rgb,
    pub accent: Rgb,
    pub shade: Rgb,
}

impl Palette {
    pub fn from_image(image: &RgbaImage) -> Self {
        let clusters = dominant_colors(image);
        let dominant = clusters
            .iter()
            .max_by(|a, b| a.weight.total_cmp(&b.weight))
            .map_or(Rgb::gray(0.2), |cluster| cluster.color);
        let primary = most_vibrant(&clusters, |_| true).unwrap_or(dominant);

        let shade = Hsl {
            saturation: dominant.to_hsl().saturation.min(0.45),
            lightness: 0.06,
            ..dominant.to_hsl()
        }
        .to_rgb();

        if primary.chroma() < ACHROMATIC_CHROMA {
            return Self {
                primary: Rgb::gray(0.94),
                accent: Rgb::gray(0.68),
                shade,
            };
        }

        let primary_hue = primary.to_hsl().hue;
        let accent = most_vibrant(&clusters, |color| {
            color.chroma() >= ACHROMATIC_CHROMA
                && hue_distance(color.to_hsl().hue, primary_hue) >= MIN_ACCENT_HUE_DISTANCE
        })
        .unwrap_or_else(|| {
            Hsl {
                hue: primary_hue + FALLBACK_ACCENT_HUE_SHIFT,
                ..primary.to_hsl()
            }
            .to_rgb()
        });

        Self {
            primary: vivid(primary),
            accent: vivid(accent),
            shade,
        }
    }
}

fn vivid(color: Rgb) -> Rgb {
    let hsl = color.to_hsl();
    Hsl {
        hue: hsl.hue,
        saturation: hsl.saturation.max(0.55),
        lightness: hsl.lightness.clamp(0.6, 0.74),
    }
    .to_rgb()
}

fn most_vibrant(clusters: &[Cluster], accept: impl Fn(Rgb) -> bool) -> Option<Rgb> {
    clusters
        .iter()
        .filter(|cluster| accept(cluster.color))
        .max_by(|a, b| a.vibrancy().total_cmp(&b.vibrancy()))
        .map(|cluster| cluster.color)
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let distance = (a - b).abs() % 360.0;
    distance.min(360.0 - distance)
}

fn dominant_colors(image: &RgbaImage) -> Vec<Cluster> {
    let thumbnail = imageops::resize(image, SAMPLE_SIZE, SAMPLE_SIZE, FilterType::Triangle);
    let opaque: Vec<Rgb> = thumbnail
        .pixels()
        .filter(|pixel| pixel[3] >= 128)
        .map(Rgb::from_pixel)
        .collect();
    let samples = if opaque.is_empty() {
        thumbnail.pixels().map(Rgb::from_pixel).collect()
    } else {
        opaque
    };
    kmeans(&samples)
}

fn kmeans(samples: &[Rgb]) -> Vec<Cluster> {
    let mut centroids = initial_centroids(samples);
    let mut counts = vec![0usize; centroids.len()];

    for _ in 0..ITERATIONS {
        let mut sums = vec![Rgb::gray(0.0); centroids.len()];
        counts.fill(0);
        for &sample in samples {
            let nearest = nearest_centroid(&centroids, sample);
            let sum = &mut sums[nearest];
            sum.r += sample.r;
            sum.g += sample.g;
            sum.b += sample.b;
            counts[nearest] += 1;
        }
        for ((centroid, sum), &count) in centroids.iter_mut().zip(&sums).zip(&counts) {
            if count > 0 {
                let scale = 1.0 / count as f32;
                *centroid = Rgb {
                    r: sum.r * scale,
                    g: sum.g * scale,
                    b: sum.b * scale,
                };
            }
        }
    }

    centroids
        .into_iter()
        .zip(counts)
        .filter(|&(_, count)| count > 0)
        .map(|(color, count)| Cluster {
            color,
            weight: count as f32 / samples.len() as f32,
        })
        .collect()
}

fn initial_centroids(samples: &[Rgb]) -> Vec<Rgb> {
    let Some(&first) = samples.first() else {
        return Vec::new();
    };
    let mut centroids = vec![first];
    while centroids.len() < CLUSTER_COUNT {
        let farthest = samples
            .iter()
            .map(|&sample| (sample, min_distance(&centroids, sample)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match farthest {
            Some((sample, distance)) if distance > 0.0 => centroids.push(sample),
            _ => break,
        }
    }
    centroids
}

fn nearest_centroid(centroids: &[Rgb], sample: Rgb) -> usize {
    centroids
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.distance_squared(sample).total_cmp(&b.1.distance_squared(sample)))
        .map_or(0, |(index, _)| index)
}

fn min_distance(centroids: &[Rgb], sample: Rgb) -> f32 {
    centroids
        .iter()
        .map(|centroid| centroid.distance_squared(sample))
        .fold(f32::INFINITY, f32::min)
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

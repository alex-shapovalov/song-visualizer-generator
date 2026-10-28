mod audio;
mod beats;
mod dsp;
mod encoder;
mod graphics;
mod math;
mod palette;
mod particles;
mod scene;
mod smoke;
mod text;
mod timeline;

use std::path::PathBuf;
use std::sync::mpsc::sync_channel;
use std::sync::{Mutex, PoisonError};
use std::thread;

use anyhow::{Context, Result, anyhow};
use image::ImageReader;
use rayon::prelude::*;
use tiny_skia::Pixmap;

use crate::jobs::{Job, Stage};
use encoder::Encoder;
use palette::Palette;
use scene::Scene;
use timeline::Timeline;

pub use encoder::ensure_ffmpeg;
pub use text::TextRenderer;

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
pub const FPS: u32 = 30;

const ANALYSIS_SAMPLE_RATE: u32 = 44_100;

pub struct RenderRequest {
    pub title: String,
    pub artist: String,
    pub art_path: PathBuf,
    pub audio_path: PathBuf,
    pub output_path: PathBuf,
}

pub fn render_video(request: &RenderRequest, text: &Mutex<TextRenderer>, job: &Job) -> Result<()> {
    job.set_stage(Stage::Analyzing);

    let samples = audio::decode_mono(&request.audio_path, ANALYSIS_SAMPLE_RATE)?;
    let timeline = Timeline::analyze(&samples, ANALYSIS_SAMPLE_RATE, FPS, scene::BAR_COUNT);
    tracing::info!(beats = timeline.beat_count(), "Detected beats");
    drop(samples);

    let art = ImageReader::open(&request.art_path)
        .context("Could not open the album art")?
        .with_guessed_format()
        .context("Could not read the album art")?
        .decode()
        .context("The album art is not a supported image (use JPEG, PNG or WebP)")?
        .into_rgba8();
    let palette = Palette::from_image(&art);

    let scene = {
        let mut text = text.lock().unwrap_or_else(PoisonError::into_inner);
        Scene::build(&art, &palette, &request.title, &request.artist, &mut text)?
    };
    drop(art);

    job.set_frame_total(timeline.frame_count());
    job.set_stage(Stage::Rendering);

    let mut encoder = Encoder::spawn(&request.audio_path, &request.output_path)?;
    let streamed = stream_frames(&scene, &timeline, &mut encoder, job);
    job.set_stage(Stage::Finalizing);
    encoder.finish()?;
    streamed
}

fn stream_frames(scene: &Scene, timeline: &Timeline, encoder: &mut Encoder, job: &Job) -> Result<()> {
    let frame_count = timeline.frame_count();
    let batch_size = rayon::current_num_threads() * 2;
    let (sender, receiver) = sync_channel::<Vec<Pixmap>>(1);

    thread::scope(|scope| {
        let writer = scope.spawn(move || -> Result<()> {
            for batch in receiver {
                for frame in batch {
                    encoder.write_frame(frame.data())?;
                    job.frame_done();
                }
            }
            Ok(())
        });

        for start in (0..frame_count).step_by(batch_size) {
            let end = (start + batch_size).min(frame_count);
            let batch = (start..end)
                .into_par_iter()
                .map(|index| scene.render_frame(&timeline.frame(index)))
                .collect();
            if sender.send(batch).is_err() {
                break;
            }
        }
        drop(sender);

        writer.join().map_err(|_| anyhow!("The frame writer crashed"))?
    })
}

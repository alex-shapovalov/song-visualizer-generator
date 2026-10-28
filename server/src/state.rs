use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Instant;

use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::jobs::{Job, Stage};
use crate::render::{self, RenderRequest, TextRenderer};

const OUTPUT_FILE_NAME: &str = "visualizer.mp4";

pub struct JobInput {
    pub title: String,
    pub artist: String,
    pub art_path: PathBuf,
    pub audio_path: PathBuf,
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    work_dir: PathBuf,
    jobs: RwLock<HashMap<Uuid, Arc<Job>>>,
    render_slot: Semaphore,
    text: Mutex<TextRenderer>,
}

impl AppState {
    pub fn new(work_dir: PathBuf, text: TextRenderer) -> Self {
        Self {
            inner: Arc::new(Inner {
                work_dir,
                jobs: RwLock::new(HashMap::new()),
                render_slot: Semaphore::new(1),
                text: Mutex::new(text),
            }),
        }
    }

    pub fn job_dir(&self, id: Uuid) -> PathBuf {
        self.inner.work_dir.join(id.to_string())
    }

    pub fn job(&self, id: Uuid) -> Option<Arc<Job>> {
        self.inner
            .jobs
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&id)
            .cloned()
    }

    pub fn start_job(&self, id: Uuid, input: JobInput) {
        let output_path = self.job_dir(id).join(OUTPUT_FILE_NAME);
        let job = Arc::new(Job::new(output_path.clone(), download_name(&input.artist, &input.title)));
        self.inner
            .jobs
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, Arc::clone(&job));

        let request = RenderRequest {
            title: input.title,
            artist: input.artist,
            art_path: input.art_path,
            audio_path: input.audio_path,
            output_path,
        };
        tokio::spawn(self.clone().run_job(id, job, request));
    }

    async fn run_job(self, id: Uuid, job: Arc<Job>, request: RenderRequest) {
        let Ok(_permit) = self.inner.render_slot.acquire().await else {
            job.fail("The server is shutting down".to_owned());
            return;
        };

        let started = Instant::now();
        let state = self.clone();
        let render_job = Arc::clone(&job);
        let outcome = tokio::task::spawn_blocking(move || {
            let result = render::render_video(&request, &state.inner.text, &render_job);
            remove_inputs(&request);
            result
        })
        .await;

        match outcome {
            Ok(Ok(())) => {
                job.set_stage(Stage::Done);
                tracing::info!(%id, elapsed = ?started.elapsed(), "Render finished");
            }
            Ok(Err(error)) => {
                tracing::error!(%id, "Render failed: {error:#}");
                job.fail(format!("{error:#}"));
            }
            Err(error) => {
                tracing::error!(%id, "Render task panicked: {error}");
                job.fail("Rendering crashed unexpectedly".to_owned());
            }
        }
    }
}

fn remove_inputs(request: &RenderRequest) {
    for path in [&request.art_path, &request.audio_path] {
        if let Err(error) = std::fs::remove_file(path) {
            tracing::warn!("Could not remove {}: {error}", path.display());
        }
    }
}

fn download_name(artist: &str, title: &str) -> String {
    let name: String = format!("{artist} - {title}")
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    format!("{}.mp4", name.trim())
}

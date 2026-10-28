use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Queued,
    Analyzing,
    Rendering,
    Finalizing,
    Done,
    Failed,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    stage: Stage,
    progress: f64,
    error: Option<String>,
    file_name: String,
}

struct JobState {
    stage: Stage,
    error: Option<String>,
}

pub struct Job {
    output_path: PathBuf,
    file_name: String,
    state: Mutex<JobState>,
    frames_done: AtomicU64,
    frames_total: AtomicU64,
}

impl Job {
    pub fn new(output_path: PathBuf, file_name: String) -> Self {
        Self {
            output_path,
            file_name,
            state: Mutex::new(JobState {
                stage: Stage::Queued,
                error: None,
            }),
            frames_done: AtomicU64::new(0),
            frames_total: AtomicU64::new(0),
        }
    }

    pub fn output_path(&self) -> &PathBuf {
        &self.output_path
    }

    pub fn set_stage(&self, stage: Stage) {
        self.state().stage = stage;
    }

    pub fn fail(&self, error: String) {
        let mut state = self.state();
        state.stage = Stage::Failed;
        state.error = Some(error);
    }

    pub fn set_frame_total(&self, total: usize) {
        self.frames_total.store(total as u64, Ordering::Relaxed);
    }

    pub fn frame_done(&self) {
        self.frames_done.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_done(&self) -> bool {
        self.state().stage == Stage::Done
    }

    pub fn status(&self) -> JobStatus {
        let state = self.state();
        JobStatus {
            stage: state.stage,
            progress: self.progress(state.stage),
            error: state.error.clone(),
            file_name: self.file_name.clone(),
        }
    }

    fn progress(&self, stage: Stage) -> f64 {
        match stage {
            Stage::Done => 1.0,
            Stage::Rendering | Stage::Finalizing => {
                let total = self.frames_total.load(Ordering::Relaxed);
                if total == 0 {
                    return 0.0;
                }
                self.frames_done.load(Ordering::Relaxed) as f64 / total as f64
            }
            Stage::Queued | Stage::Analyzing | Stage::Failed => 0.0,
        }
    }

    fn state(&self) -> MutexGuard<'_, JobState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

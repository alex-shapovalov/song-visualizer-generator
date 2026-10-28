use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::multipart::{Field, MultipartError};
use axum::extract::{DefaultBodyLimit, Multipart, Path as UrlPath, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

use crate::jobs::JobStatus;
use crate::state::{AppState, JobInput};

const MAX_UPLOAD_BYTES: usize = 1024 * 1024 * 1024;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/{id}", get(job_status))
        .route("/api/jobs/{id}/video", get(job_video))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
        .with_state(state)
}

#[derive(Serialize)]
struct CreatedJob {
    id: Uuid,
}

async fn create_job(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<CreatedJob>), ApiError> {
    let id = Uuid::new_v4();
    let job_dir = state.job_dir(id);
    tokio::fs::create_dir_all(&job_dir).await?;

    let input = match read_upload(&mut multipart, &job_dir).await {
        Ok(input) => input,
        Err(error) => {
            if let Err(cleanup_error) = tokio::fs::remove_dir_all(&job_dir).await {
                tracing::warn!("Could not remove {}: {cleanup_error}", job_dir.display());
            }
            return Err(error);
        }
    };

    state.start_job(id, input);
    Ok((StatusCode::ACCEPTED, Json(CreatedJob { id })))
}

async fn job_status(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<Uuid>,
) -> Result<Json<JobStatus>, ApiError> {
    let job = state.job(id).ok_or(ApiError::NotFound)?;
    Ok(Json(job.status()))
}

async fn job_video(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let job = state.job(id).ok_or(ApiError::NotFound)?;
    if !job.is_done() {
        return Err(ApiError::NotReady);
    }
    let response = ServeFile::new(job.output_path()).oneshot(request).await?;
    Ok(response.map(Body::new))
}

async fn read_upload(multipart: &mut Multipart, dir: &Path) -> Result<JobInput, ApiError> {
    let mut title = None;
    let mut artist = None;
    let mut art_path = None;
    let mut audio_path = None;

    while let Some(mut field) = multipart.next_field().await? {
        let name = field.name().unwrap_or_default().to_owned();
        match name.as_str() {
            "title" => title = Some(field.text().await?),
            "artist" => artist = Some(field.text().await?),
            "art" => art_path = Some(save_field(&mut field, dir.join("art")).await?),
            "audio" => audio_path = Some(save_field(&mut field, dir.join("audio")).await?),
            _ => {}
        }
    }

    Ok(JobInput {
        title: required_text(title, "Song name")?,
        artist: required_text(artist, "Artist name")?,
        art_path: art_path.ok_or_else(|| ApiError::BadRequest("Album art is required".to_owned()))?,
        audio_path: audio_path.ok_or_else(|| ApiError::BadRequest("Audio file is required".to_owned()))?,
    })
}

fn required_text(value: Option<String>, label: &str) -> Result<String, ApiError> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| ApiError::BadRequest(format!("{label} is required")))
}

async fn save_field(field: &mut Field<'_>, path: PathBuf) -> Result<PathBuf, ApiError> {
    let mut file = tokio::fs::File::create(&path).await?;
    while let Some(chunk) = field.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(path)
}

pub enum ApiError {
    BadRequest(String),
    NotFound,
    NotReady,
    Upload(MultipartError),
    Internal(anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::NotFound => (StatusCode::NOT_FOUND, "Job not found".to_owned()),
            Self::NotReady => (StatusCode::CONFLICT, "The video is not ready yet".to_owned()),
            Self::Upload(error) => (error.status(), error.body_text()),
            Self::Internal(error) => {
                tracing::error!("Request failed: {error:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_owned())
            }
        };
        (status, Json(ErrorBody { error: message })).into_response()
    }
}

impl From<MultipartError> for ApiError {
    fn from(error: MultipartError) -> Self {
        Self::Upload(error)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl From<std::convert::Infallible> for ApiError {
    fn from(error: std::convert::Infallible) -> Self {
        match error {}
    }
}

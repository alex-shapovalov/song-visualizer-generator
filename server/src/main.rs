mod api;
mod jobs;
mod render;
mod state;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;

use anyhow::{Context, Result};

use crate::render::TextRenderer;
use crate::state::AppState;

const PORT: u16 = 8787;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    render::ensure_ffmpeg()?;

    let work_dir = std::env::temp_dir().join("song-visualizer");
    reset_work_dir(&work_dir)?;

    let text = tokio::task::spawn_blocking(TextRenderer::new).await?;
    let state = AppState::new(work_dir, text);

    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, PORT));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("Could not bind to {address}"))?;

    tracing::info!("Visualizer server listening on http://{address}");
    axum::serve(listener, api::router(state)).await?;
    Ok(())
}

fn reset_work_dir(work_dir: &Path) -> Result<()> {
    if work_dir.exists() {
        std::fs::remove_dir_all(work_dir)
            .with_context(|| format!("Could not clear {}", work_dir.display()))?;
    }
    std::fs::create_dir_all(work_dir)
        .with_context(|| format!("Could not create {}", work_dir.display()))
}

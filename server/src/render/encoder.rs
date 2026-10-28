use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, ensure};

use super::{FPS, HEIGHT, WIDTH};

pub fn ensure_ffmpeg() -> Result<()> {
    let available = Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    ensure!(
        available,
        "ffmpeg was not found on PATH. Install it (e.g. `brew install ffmpeg`) and restart the server."
    );
    Ok(())
}

pub struct Encoder {
    child: Child,
    stdin: ChildStdin,
    stderr: JoinHandle<String>,
}

impl Encoder {
    pub fn spawn(audio_path: &Path, output_path: &Path) -> Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "rawvideo", "-pixel_format", "rgba"])
            .args(["-video_size", &format!("{WIDTH}x{HEIGHT}")])
            .args(["-framerate", &FPS.to_string()])
            .args(["-i", "pipe:0", "-i"])
            .arg(audio_path)
            .args(["-map", "0:v:0", "-map", "1:a:0"])
            .args(["-vf", "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p"])
            .args(["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-profile:v", "high", "-level", "5.1"])
            .args(["-g", &(FPS / 2).to_string(), "-bf", "2"])
            .args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"])
            .args(["-c:a", "aac", "-b:a", "320k"])
            .args(["-shortest", "-movflags", "+faststart"])
            .arg(output_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to start ffmpeg")?;

        let stdin = child.stdin.take().context("ffmpeg stdin is unavailable")?;
        let mut stderr = child.stderr.take().context("ffmpeg stderr is unavailable")?;
        let stderr = thread::spawn(move || {
            let mut log = Vec::new();
            if let Err(error) = stderr.read_to_end(&mut log) {
                return format!("Could not read ffmpeg output: {error}");
            }
            String::from_utf8_lossy(&log).into_owned()
        });

        Ok(Self { child, stdin, stderr })
    }

    pub fn write_frame(&mut self, rgba: &[u8]) -> Result<()> {
        self.stdin
            .write_all(rgba)
            .context("ffmpeg stopped accepting frames")
    }

    pub fn finish(self) -> Result<()> {
        let Self { mut child, stdin, stderr } = self;
        drop(stdin);
        let status = child.wait().context("Failed to wait for ffmpeg")?;
        let log = stderr.join().unwrap_or_default();
        ensure!(status.success(), "ffmpeg failed to encode the video: {}", log.trim());
        Ok(())
    }
}

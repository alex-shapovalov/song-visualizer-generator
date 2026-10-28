# Song Visualizer Generator

A local app that turns a song and its cover into a 4K visualizer video.

You give it a title, an artist name, album art, and an audio file. It renders an MP4 with the cover, those names, and an 80-bar spectrum colored from the artwork. Smoke and particles move with the energy of the track, and the cover pulses on the beat. You can preview the video in the browser and download it.

Output is 3840×2160 at 30 fps. Video is H.264 (CRF 18, High profile, level 5.1, BT.709). Audio is AAC at 320 kbps. The file is written with `faststart`, so a player can start it before the download finishes.

Rendering is CPU-heavy, and a full song takes a while. The server runs one render at a time. Extra jobs wait until the current one finishes.

## Requirements

- [Rust](https://rustup.rs/) 1.85 or newer (`server` uses the 2024 edition)
- [Node.js](https://nodejs.org/) 20.19+ or 22.12+ (Node 21 is outside the range the frontend tooling accepts)
- [ffmpeg](https://ffmpeg.org/) on your `PATH`, with libx264 and an AAC encoder. A normal build includes both.

```bash
# macOS
brew install ffmpeg

# Debian, Ubuntu
sudo apt install ffmpeg
```

## Run

```bash
npm run setup
npm run dev
```

Then open [http://localhost:5173](http://localhost:5173).

`npm run dev` starts the Rust server in release mode and the Vite dev server together. The first Rust build is slow. The API listens on `http://127.0.0.1:8787`, and Vite proxies `/api` to it. The server only binds to localhost, and it has no authentication.

## What to upload

| Field | Accepted | Notes |
| --- | --- | --- |
| Song name | Text | Drawn on the video |
| Artist name | Text | Drawn on the video. The download is named `Artist - Title.mp4` |
| Album art | JPEG, PNG, WebP | Bar colors and the blurred background come from this image |
| Audio | WAV, AIFF | Decoded with ffmpeg |

The whole upload can be up to 1 GB.

Uploads and finished videos are stored in your OS temp directory, in a folder named `song-visualizer`. Starting the server deletes that folder, so a restart throws away anything already rendered.

## Layout

```
web/                 React and Vite frontend
server/              Axum API and the renderer
server/src/render/   Spectrum analysis, scene drawing, ffmpeg encode
server/assets/fonts/ Inter, used for the title and artist
```

Frames are drawn with [tiny-skia](https://github.com/linebender/tiny-skia) and piped to ffmpeg as raw RGBA. Audio analysis uses [rustfft](https://github.com/ejmahler/rustfft). Text is set with [cosmic-text](https://github.com/pop-os/cosmic-text).

The API is three routes:

- `POST /api/jobs` takes multipart fields `title`, `artist`, `art`, and `audio`, and returns `{ "id" }`.
- `GET /api/jobs/{id}` returns the stage (`queued`, `analyzing`, `rendering`, `finalizing`, `done`, or `failed`), progress, and the download file name.
- `GET /api/jobs/{id}/video` returns the MP4 once the job is done.

## Fonts

The Inter files in `server/assets/fonts` are under the [SIL Open Font License](server/assets/fonts/Inter-LICENSE.txt).

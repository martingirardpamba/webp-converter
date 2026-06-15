# WebP Converter

Convert images to WebP and heavy videos to web-light MP4/WebM in one click. Drop a folder, hit convert, done.

**No command line. The image converter stays tiny; video conversion bundles FFmpeg.**

![WebP Converter screenshot](screenshot.png)

## Download

**[Download for Windows (2 MB)](https://github.com/martingirardpamba/webp-converter/releases/latest)**

Download the `.exe` installer, run it, that's it.

## What it does

**Images** — converts JPG, PNG, GIF, BMP, TIFF to WebP, resizes to a max width (default 1920px, no upscale), writes to a `webp/` folder next to the originals.

**Videos** — converts MP4, MOV, AVI, MKV, WebM and more to web-light **MP4 (H.264)** or **WebM (VP9)** using constant-quality (CRF) encoding — visually lossless, much smaller. Caps height (default 1080p), optional audio removal, writes to a `web/` folder next to the originals. Originals are never touched.

## Why WebP?

WebP images are **50-90% smaller** than JPG/PNG with similar quality. Your website loads faster, your storage costs drop.

## Settings

**Images**

| Setting | Default | What it does |
|---------|---------|-------------|
| Quality | 80 | WebP quality (1-100). 80 is a good balance. |
| Max width | 1920px | Images wider than this get resized down. |
| Subfolders | On | Process images in all subfolders too. |

**Videos**

| Setting | Default | What it does |
|---------|---------|-------------|
| Format | MP4 (H.264) | MP4 = compatible everywhere; WebM (VP9) = ~30% lighter, slower. |
| Quality | 80 | Maps to CRF (constant quality). Higher = better/larger. |
| Max height | 1080px | Videos taller than this get scaled down (no upscale). |
| Silencieux | Off | Removes the audio track (useful for background loops). |
| Subfolders | On | Process videos in all subfolders too. |

## Build from source

Requires [Rust](https://rustup.rs), [Node.js](https://nodejs.org), and PowerShell (Windows) or Bash (macOS/Linux).

```
git clone https://github.com/magipa-consulting/webp-converter.git
cd webp-converter
npm install

# Fetch the bundled FFmpeg binary for your platform:
pwsh -File scripts/fetch-ffmpeg.ps1        # Windows
# or: ./scripts/fetch-ffmpeg.sh            # macOS / Linux

npx tauri build
```

Installer will be in `src-tauri/target/release/bundle/`.

> The video feature bundles a GPL build of FFmpeg — see `NOTICE.md`.

## License

MIT — do whatever you want with it.

Made by [MAGIPA Consulting](https://magipa.fr)

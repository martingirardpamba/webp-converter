# Video Conversion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Vidéos (Web)" mode to WebP Converter that batch-converts heavy videos into web-light MP4 (H.264) or WebM (VP9) using constant-quality (CRF) encoding, via a bundled FFmpeg sidecar.

**Architecture:** A new isolated Rust module `video.rs` (pure functions + scan) drives FFmpeg through Tauri's `tauri-plugin-shell` sidecar. The existing image converter (`converter.rs`) is untouched. The frontend gains a tab toggle; image flow is preserved, a parallel video flow is added with per-file progress and cancellation.

**Tech Stack:** Tauri v2, Rust (`tauri-plugin-shell`, `walkdir`, `serde`), bundled FFmpeg (GPL static build with libx264 + libvpx-vp9 + aac/opus), vanilla HTML/JS/CSS.

**Reference spec:** `docs/superpowers/specs/2026-06-15-video-conversion-design.md`

**Working branch:** `feature/video-conversion` (already created).

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `src-tauri/Cargo.toml` | Modify | Add `tauri-plugin-shell`, dev-dep `tempfile` |
| `src-tauri/src/video.rs` | Create | Video scan + pure functions (CRF, args, parsing) + structs |
| `src-tauri/src/lib.rs` | Modify | Register shell plugin, `VideoState`, 3 new commands |
| `src-tauri/tauri.conf.json` | Modify | `bundle.externalBin` |
| `src-tauri/capabilities/default.json` | Modify | `shell:allow-execute` for ffmpeg sidecar |
| `src-tauri/tests/video_integration.rs` | Create | ffmpeg-gated end-to-end encode test |
| `scripts/fetch-ffmpeg.ps1` | Create | Download Windows ffmpeg binary into `binaries/` |
| `scripts/fetch-ffmpeg.sh` | Create | Download macOS/Linux ffmpeg binary |
| `.gitignore` | Modify | Ignore `src-tauri/binaries/` |
| `src/index.html` | Modify | Tab toggle + video settings panel + cancel button |
| `src/style.css` | Modify | Tabs, select, hint, cancel button styles |
| `src/main.js` | Modify | Tab switching, video scan/convert/cancel, video events |
| `README.md` | Modify | Reposition size/dependency claims, build steps |
| `NOTICE.md` | Create | FFmpeg GPL bundling notice |

---

## Task 1: Dependencies + module skeleton

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/video.rs`
- Modify: `src-tauri/src/lib.rs:1` (add `pub mod video;`)

- [ ] **Step 1: Add dependencies to `Cargo.toml`**

In `[dependencies]`, after the `webp = "0.3"` line, add:

```toml
tauri-plugin-shell = "2"
```

After the `[build-dependencies]` block, add a new section:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create `src-tauri/src/video.rs` with constants, format enum and structs**

```rust
use serde::Serialize;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "avi", "mkv", "webm", "m4v", "flv", "wmv", "mpg", "mpeg", "m2ts", "3gp",
];
pub const VIDEO_OUTPUT_DIR: &str = "web";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoFormat {
    Mp4,
    Webm,
}

impl VideoFormat {
    /// Lenient parse: anything that isn't "webm" falls back to MP4.
    pub fn from_str_lenient(s: &str) -> VideoFormat {
        match s.to_lowercase().as_str() {
            "webm" => VideoFormat::Webm,
            _ => VideoFormat::Mp4,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            VideoFormat::Mp4 => "mp4",
            VideoFormat::Webm => "webm",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoScanResult {
    pub files: Vec<String>,
    pub total_size: u64,
    pub folder_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoProgress {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
    pub file_percent: u8, // 0..=100 within the current file
    pub size_before: u64,
    pub size_after: u64,
    pub status: String, // "encoding", "ok", "skip", "error"
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoReport {
    pub converted: usize,
    pub skipped: usize,
    pub errors: usize,
    pub cancelled: bool,
    pub total_size_before: u64,
    pub total_size_after: u64,
    pub first_output_dir: Option<String>,
}
```

- [ ] **Step 3: Register the module in `lib.rs`**

Change line 1 of `src-tauri/src/lib.rs` from:

```rust
mod converter;
```

to:

```rust
mod converter;
pub mod video;
```

- [ ] **Step 4: Verify it compiles**

Run: `cd src-tauri && cargo build`
Expected: builds successfully (warnings about unused items are OK at this stage).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/video.rs src-tauri/src/lib.rs
git commit -m "Add video module skeleton and shell plugin dependency"
```

---

## Task 2: `is_video_file` (TDD)

**Files:**
- Modify: `src-tauri/src/video.rs` (add function + test module)

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/video.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_video_extensions_case_insensitive() {
        assert!(is_video_file(Path::new("a.mp4")));
        assert!(is_video_file(Path::new("A.MOV")));
        assert!(is_video_file(Path::new("clip.MKV")));
        assert!(!is_video_file(Path::new("photo.jpg")));
        assert!(!is_video_file(Path::new("noext")));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib detects_video_extensions`
Expected: FAIL — `cannot find function 'is_video_file'`.

- [ ] **Step 3: Write minimal implementation**

Add to `src-tauri/src/video.rs` (above the `#[cfg(test)]` module):

```rust
pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib detects_video_extensions`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/video.rs
git commit -m "Add is_video_file with tests"
```

---

## Task 3: `output_path` derivation (TDD)

**Files:**
- Modify: `src-tauri/src/video.rs`

- [ ] **Step 1: Write the failing test**

Add inside the `tests` module in `src-tauri/src/video.rs`:

```rust
    #[test]
    fn derives_output_path_in_web_subdir() {
        let out = output_path(Path::new("C:/clips/holiday.mov"), VideoFormat::Mp4).unwrap();
        let s = out.to_string_lossy().replace('\\', "/");
        assert_eq!(s, "C:/clips/web/holiday.mp4");
    }

    #[test]
    fn derives_webm_output_path() {
        let out = output_path(Path::new("/videos/a.mp4"), VideoFormat::Webm).unwrap();
        let s = out.to_string_lossy().replace('\\', "/");
        assert_eq!(s, "/videos/web/a.webm");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib output_path`
Expected: FAIL — `cannot find function 'output_path'`.

- [ ] **Step 3: Write minimal implementation**

Add to `src-tauri/src/video.rs` (above the tests module):

```rust
pub fn output_path(src: &Path, format: VideoFormat) -> Option<PathBuf> {
    let parent = src.parent()?;
    let stem = src.file_stem()?;
    let out_dir = parent.join(VIDEO_OUTPUT_DIR);
    Some(out_dir.join(format!("{}.{}", stem.to_string_lossy(), format.extension())))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib output_path`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/video.rs
git commit -m "Add output_path derivation with tests"
```

---

## Task 4: `quality_to_crf` (TDD)

**Files:**
- Modify: `src-tauri/src/video.rs`

- [ ] **Step 1: Write the failing test**

Add inside the `tests` module:

```rust
    #[test]
    fn maps_quality_to_h264_crf() {
        assert_eq!(quality_to_crf(100, VideoFormat::Mp4), 18);
        assert_eq!(quality_to_crf(80, VideoFormat::Mp4), 21);
        assert_eq!(quality_to_crf(50, VideoFormat::Mp4), 26);
        assert_eq!(quality_to_crf(1, VideoFormat::Mp4), 34);
    }

    #[test]
    fn maps_quality_to_vp9_crf() {
        assert_eq!(quality_to_crf(100, VideoFormat::Webm), 24);
        assert_eq!(quality_to_crf(80, VideoFormat::Webm), 27);
        assert_eq!(quality_to_crf(50, VideoFormat::Webm), 32);
        assert_eq!(quality_to_crf(1, VideoFormat::Webm), 40);
    }

    #[test]
    fn clamps_out_of_range_quality() {
        assert_eq!(quality_to_crf(0, VideoFormat::Mp4), 34);   // clamps up to 1
        assert_eq!(quality_to_crf(200, VideoFormat::Mp4), 18); // clamps down to 100
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib quality_to_crf`
Expected: FAIL — `cannot find function 'quality_to_crf'`.

- [ ] **Step 3: Write minimal implementation**

Add to `src-tauri/src/video.rs`:

```rust
/// Maps a 1..=100 quality slider to a codec CRF (lower CRF = higher quality).
/// H.264 visually-lossless ~18, web-good ~21-26.
/// VP9 web-good ~24-32.
pub fn quality_to_crf(quality: u32, format: VideoFormat) -> u8 {
    let q = quality.clamp(1, 100) as f64;
    match format {
        VideoFormat::Mp4 => (34.0 - q * 0.16).round().clamp(18.0, 34.0) as u8,
        VideoFormat::Webm => (40.0 - q * 0.16).round().clamp(24.0, 40.0) as u8,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib quality_to_crf`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/video.rs
git commit -m "Add quality_to_crf mapping with tests"
```

---

## Task 5: Progress parsing (`parse_duration`, `parse_out_time`) (TDD)

**Files:**
- Modify: `src-tauri/src/video.rs`

- [ ] **Step 1: Write the failing test**

Add inside the `tests` module:

```rust
    #[test]
    fn parses_duration_line() {
        let line = "  Duration: 00:01:23.45, start: 0.000000, bitrate: 1234 kb/s";
        let d = parse_duration(line).unwrap();
        assert!((d - 83.45).abs() < 0.01, "got {d}");
    }

    #[test]
    fn ignores_non_duration_lines() {
        assert!(parse_duration("frame= 10 fps=0.0").is_none());
    }

    #[test]
    fn parses_out_time_us() {
        let t = parse_out_time("out_time_us=12340000").unwrap();
        assert!((t - 12.34).abs() < 0.001, "got {t}");
    }

    #[test]
    fn parses_out_time_hms() {
        let t = parse_out_time("out_time=00:00:12.340000").unwrap();
        assert!((t - 12.34).abs() < 0.001, "got {t}");
    }

    #[test]
    fn out_time_na_returns_none() {
        assert!(parse_out_time("out_time_us=N/A").is_none());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib parse`
Expected: FAIL — `cannot find function 'parse_duration'`.

- [ ] **Step 3: Write minimal implementation**

Add to `src-tauri/src/video.rs`:

```rust
fn parse_hms(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: f64 = parts[0].trim().parse().ok()?;
    let m: f64 = parts[1].trim().parse().ok()?;
    let sec: f64 = parts[2].trim().parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Parses ffmpeg's stderr `Duration: HH:MM:SS.ss, ...` line into seconds.
pub fn parse_duration(line: &str) -> Option<f64> {
    let idx = line.find("Duration:")?;
    let rest = line[idx + "Duration:".len()..].trim_start();
    let time_str = rest.split(',').next()?.trim();
    parse_hms(time_str)
}

/// Parses a `-progress pipe:1` line into elapsed output seconds.
/// Handles `out_time_us=` (microseconds), `out_time_ms=` (microseconds, ffmpeg quirk)
/// and `out_time=HH:MM:SS.ss`.
pub fn parse_out_time(line: &str) -> Option<f64> {
    let line = line.trim();
    if let Some(v) = line.strip_prefix("out_time_us=") {
        let us: f64 = v.trim().parse().ok()?;
        return Some(us / 1_000_000.0);
    }
    if let Some(v) = line.strip_prefix("out_time_ms=") {
        let us: f64 = v.trim().parse().ok()?;
        return Some(us / 1_000_000.0);
    }
    if let Some(v) = line.strip_prefix("out_time=") {
        return parse_hms(v.trim());
    }
    None
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib parse`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/video.rs
git commit -m "Add ffmpeg duration/progress parsing with tests"
```

---

## Task 6: `build_ffmpeg_args` (TDD)

**Files:**
- Modify: `src-tauri/src/video.rs`

- [ ] **Step 1: Write the failing test**

Add inside the `tests` module:

```rust
    #[test]
    fn builds_h264_args_with_faststart_and_audio() {
        let args = build_ffmpeg_args("in.mov", "out.mp4", VideoFormat::Mp4, 21, 1080, false);
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.windows(2).any(|w| w[0] == "-crf" && w[1] == "21"));
        assert!(args.windows(2).any(|w| w[0] == "-movflags" && w[1] == "+faststart"));
        assert!(args.windows(2).any(|w| w[0] == "-c:a" && w[1] == "aac"));
        assert!(args.contains(&"scale=-2:min(1080\\,ih)".to_string()));
        assert!(args.windows(2).any(|w| w[0] == "-progress" && w[1] == "pipe:1"));
        let i_in = args.iter().position(|a| a == "in.mov").unwrap();
        let i_out = args.iter().position(|a| a == "out.mp4").unwrap();
        assert!(i_in < i_out, "input must come before output");
    }

    #[test]
    fn builds_vp9_args() {
        let args = build_ffmpeg_args("in.mp4", "out.webm", VideoFormat::Webm, 27, 720, false);
        assert!(args.contains(&"libvpx-vp9".to_string()));
        assert!(args.windows(2).any(|w| w[0] == "-b:v" && w[1] == "0"));
        assert!(args.windows(2).any(|w| w[0] == "-c:a" && w[1] == "libopus"));
        assert!(!args.contains(&"+faststart".to_string()));
        assert!(args.contains(&"scale=-2:min(720\\,ih)".to_string()));
    }

    #[test]
    fn silent_uses_an_and_no_audio_codec() {
        let args = build_ffmpeg_args("in.mov", "out.mp4", VideoFormat::Mp4, 21, 1080, true);
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"aac".to_string()));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib build_ffmpeg_args` (and the three test names)
Expected: FAIL — `cannot find function 'build_ffmpeg_args'`.

- [ ] **Step 3: Write minimal implementation**

Add to `src-tauri/src/video.rs`:

```rust
/// Builds the FFmpeg argument vector for one input → one output file.
/// Args are passed to the sidecar as a single argv (no shell), so the comma
/// inside the scale `min()` expression is escaped with a backslash, which
/// ffmpeg's filtergraph parser turns back into a literal comma.
pub fn build_ffmpeg_args(
    input: &str,
    output: &str,
    format: VideoFormat,
    crf: u8,
    max_height: u32,
    silent: bool,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostats".into(),
        "-y".into(),
        "-i".into(),
        input.into(),
    ];

    match format {
        VideoFormat::Mp4 => args.extend([
            "-c:v".into(), "libx264".into(),
            "-crf".into(), crf.to_string(),
            "-preset".into(), "medium".into(),
            "-pix_fmt".into(), "yuv420p".into(),
        ]),
        VideoFormat::Webm => args.extend([
            "-c:v".into(), "libvpx-vp9".into(),
            "-crf".into(), crf.to_string(),
            "-b:v".into(), "0".into(),
            "-row-mt".into(), "1".into(),
            "-deadline".into(), "good".into(),
            "-cpu-used".into(), "2".into(),
            "-pix_fmt".into(), "yuv420p".into(),
        ]),
    }

    // Cap height, keep width even (-2), never upscale (min with ih).
    args.extend([
        "-vf".into(),
        format!("scale=-2:min({}\\,ih)", max_height),
    ]);

    if silent {
        args.push("-an".into());
    } else {
        match format {
            VideoFormat::Mp4 => args.extend([
                "-c:a".into(), "aac".into(), "-b:a".into(), "128k".into(),
            ]),
            VideoFormat::Webm => args.extend([
                "-c:a".into(), "libopus".into(), "-b:a".into(), "128k".into(),
            ]),
        }
    }

    if format == VideoFormat::Mp4 {
        args.extend(["-movflags".into(), "+faststart".into()]);
    }

    args.extend(["-progress".into(), "pipe:1".into()]);
    args.push(output.into());
    args
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib`
Expected: PASS (all video tests so far).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/video.rs
git commit -m "Add build_ffmpeg_args with tests"
```

---

## Task 7: `scan_videos` (TDD)

**Files:**
- Modify: `src-tauri/src/video.rs`

- [ ] **Step 1: Write the failing test**

Add inside the `tests` module:

```rust
    #[test]
    fn scan_skips_already_converted() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("a.mp4"), b"x").unwrap();
        std::fs::write(dir.join("b.mov"), b"xx").unwrap();
        std::fs::create_dir_all(dir.join("web")).unwrap();
        std::fs::write(dir.join("web").join("a.mp4"), b"done").unwrap();

        let res = scan_videos(dir.to_str().unwrap(), false, VideoFormat::Mp4);

        let names: Vec<String> = res
            .files
            .iter()
            .map(|f| Path::new(f).file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["b.mov"]);
        assert_eq!(res.total_size, 2);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test --lib scan_skips_already_converted`
Expected: FAIL — `cannot find function 'scan_videos'`.

- [ ] **Step 3: Write minimal implementation**

Add to `src-tauri/src/video.rs` (mirrors `converter::scan_folder`):

```rust
pub fn scan_videos(folder: &str, recursive: bool, format: VideoFormat) -> VideoScanResult {
    let mut files = Vec::new();
    let mut total_size: u64 = 0;
    let mut folders_seen = std::collections::HashSet::new();

    if recursive {
        for entry in WalkDir::new(folder).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.components().any(|c| c.as_os_str() == VIDEO_OUTPUT_DIR) {
                continue;
            }
            if path.is_file() && is_video_file(path) {
                if let (Some(parent), Some(dest)) = (path.parent(), output_path(path, format)) {
                    if !dest.exists() {
                        if let Ok(meta) = std::fs::metadata(path) {
                            total_size += meta.len();
                        }
                        folders_seen.insert(parent.to_path_buf());
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    } else {
        let dir = Path::new(folder);
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && is_video_file(&path) {
                    if let Some(dest) = output_path(&path, format) {
                        if !dest.exists() {
                            if let Ok(meta) = std::fs::metadata(&path) {
                                total_size += meta.len();
                            }
                            folders_seen.insert(dir.to_path_buf());
                            files.push(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    VideoScanResult {
        files,
        total_size,
        folder_count: folders_seen.len(),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test --lib`
Expected: PASS (all video unit tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/video.rs
git commit -m "Add scan_videos with tests"
```

---

## Task 8: Tauri commands + sidecar orchestration in `lib.rs`

**Files:**
- Modify: `src-tauri/src/lib.rs` (full replacement below)

- [ ] **Step 1: Replace `src-tauri/src/lib.rs` with the full new content**

```rust
mod converter;
pub mod video;

use std::path::Path;
use std::sync::Mutex;

use converter::{ConvertProgress, ConvertReport, ScanResult};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use video::{VideoFormat, VideoProgress, VideoReport, VideoScanResult};

#[derive(Default)]
struct VideoState {
    child: Mutex<Option<CommandChild>>,
    cancelled: Mutex<bool>,
}

#[tauri::command]
fn scan_folder(folder: String, recursive: bool) -> ScanResult {
    converter::scan_folder(&folder, recursive)
}

#[tauri::command]
fn convert(app: AppHandle, folder: String, recursive: bool, quality: f32, max_width: u32) -> ConvertReport {
    let scan = converter::scan_folder(&folder, recursive);
    let total = scan.files.len();
    let mut report = ConvertReport {
        converted: 0,
        skipped: 0,
        errors: 0,
        total_size_before: 0,
        total_size_after: 0,
        first_output_dir: None,
    };

    for (i, file_path) in scan.files.iter().enumerate() {
        let mut progress = converter::convert_image(file_path, quality, max_width);
        progress.current = i + 1;
        progress.total = total;

        match progress.status.as_str() {
            "ok" => {
                report.converted += 1;
                report.total_size_before += progress.size_before;
                report.total_size_after += progress.size_after;
                if report.first_output_dir.is_none() {
                    report.first_output_dir = webp_dir_for(file_path);
                }
            }
            "skip" => report.skipped += 1,
            _ => report.errors += 1,
        }

        let _ = app.emit("convert-progress", &progress);
    }

    let _ = app.emit("convert-done", &report);
    report
}

#[tauri::command]
fn convert_files(app: AppHandle, files: Vec<String>, quality: f32, max_width: u32) -> ConvertReport {
    let total = files.len();
    let mut report = ConvertReport {
        converted: 0,
        skipped: 0,
        errors: 0,
        total_size_before: 0,
        total_size_after: 0,
        first_output_dir: None,
    };

    for (i, file_path) in files.iter().enumerate() {
        let mut progress = converter::convert_image(file_path, quality, max_width);
        progress.current = i + 1;
        progress.total = total;

        match progress.status.as_str() {
            "ok" => {
                report.converted += 1;
                report.total_size_before += progress.size_before;
                report.total_size_after += progress.size_after;
                if report.first_output_dir.is_none() {
                    report.first_output_dir = webp_dir_for(file_path);
                }
            }
            "skip" => report.skipped += 1,
            _ => report.errors += 1,
        }

        let _ = app.emit("convert-progress", &progress);
    }

    let _ = app.emit("convert-done", &report);
    report
}

fn webp_dir_for(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .parent()
        .map(|p| p.join("webp").to_string_lossy().to_string())
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(&path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&path).spawn();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(&path).spawn();

    result.map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn scan_videos(folder: String, recursive: bool, format: String) -> VideoScanResult {
    video::scan_videos(&folder, recursive, VideoFormat::from_str_lenient(&format))
}

#[tauri::command]
fn cancel_video(state: State<'_, VideoState>) -> Result<(), String> {
    *state.cancelled.lock().unwrap() = true;
    let child = state.child.lock().unwrap().take();
    if let Some(child) = child {
        child.kill().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn convert_videos(
    app: AppHandle,
    state: State<'_, VideoState>,
    files: Vec<String>,
    format: String,
    quality: u32,
    max_height: u32,
    silent: bool,
) -> Result<VideoReport, String> {
    let fmt = VideoFormat::from_str_lenient(&format);
    *state.cancelled.lock().unwrap() = false;

    let total = files.len();
    let mut report = VideoReport {
        converted: 0,
        skipped: 0,
        errors: 0,
        cancelled: false,
        total_size_before: 0,
        total_size_after: 0,
        first_output_dir: None,
    };

    for (i, input) in files.iter().enumerate() {
        let is_cancelled = *state.cancelled.lock().unwrap();
        if is_cancelled {
            report.cancelled = true;
            break;
        }

        let in_path = Path::new(input);
        let file_name = in_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let size_before = std::fs::metadata(in_path).map(|m| m.len()).unwrap_or(0);

        let dest = match video::output_path(in_path, fmt) {
            Some(d) => d,
            None => {
                report.errors += 1;
                let _ = app.emit("video-progress", &VideoProgress {
                    current: i + 1, total, file_name, file_percent: 0,
                    size_before, size_after: 0, status: "error".into(),
                    error_message: Some("Invalid path".into()),
                });
                continue;
            }
        };

        if dest.exists() {
            report.skipped += 1;
            let _ = app.emit("video-progress", &VideoProgress {
                current: i + 1, total, file_name, file_percent: 100,
                size_before, size_after: 0, status: "skip".into(), error_message: None,
            });
            continue;
        }

        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                report.errors += 1;
                let _ = app.emit("video-progress", &VideoProgress {
                    current: i + 1, total, file_name, file_percent: 0,
                    size_before, size_after: 0, status: "error".into(),
                    error_message: Some(format!("Cannot create output dir: {e}")),
                });
                continue;
            }
        }

        let crf = video::quality_to_crf(quality, fmt);
        let dest_str = dest.to_string_lossy().to_string();
        let args = video::build_ffmpeg_args(input, &dest_str, fmt, crf, max_height, silent);

        let cmd = match app.shell().sidecar("ffmpeg") {
            Ok(c) => c.args(args),
            Err(e) => {
                report.errors += 1;
                let _ = app.emit("video-progress", &VideoProgress {
                    current: i + 1, total, file_name, file_percent: 0,
                    size_before, size_after: 0, status: "error".into(),
                    error_message: Some(format!("ffmpeg not available: {e}")),
                });
                continue;
            }
        };

        let (mut rx, child) = match cmd.spawn() {
            Ok(v) => v,
            Err(e) => {
                report.errors += 1;
                let _ = app.emit("video-progress", &VideoProgress {
                    current: i + 1, total, file_name, file_percent: 0,
                    size_before, size_after: 0, status: "error".into(),
                    error_message: Some(format!("ffmpeg spawn failed: {e}")),
                });
                continue;
            }
        };
        *state.child.lock().unwrap() = Some(child);

        let _ = app.emit("video-progress", &VideoProgress {
            current: i + 1, total, file_name: file_name.clone(), file_percent: 0,
            size_before, size_after: 0, status: "encoding".into(), error_message: None,
        });

        let mut total_secs = 0f64;
        let mut last_pct = 0u8;
        let mut tail: Vec<String> = Vec::new();
        let mut code: Option<i32> = None;

        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    if total_secs == 0.0 {
                        if let Some(d) = video::parse_duration(&line) {
                            total_secs = d;
                        }
                    }
                    tail.push(line.into_owned());
                    if tail.len() > 12 {
                        tail.remove(0);
                    }
                }
                CommandEvent::Stdout(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    if let Some(t) = video::parse_out_time(&line) {
                        if total_secs > 0.0 {
                            let pct = ((t / total_secs) * 100.0).clamp(0.0, 100.0) as u8;
                            if pct != last_pct {
                                last_pct = pct;
                                let _ = app.emit("video-progress", &VideoProgress {
                                    current: i + 1, total, file_name: file_name.clone(),
                                    file_percent: pct, size_before, size_after: 0,
                                    status: "encoding".into(), error_message: None,
                                });
                            }
                        }
                    }
                }
                CommandEvent::Terminated(payload) => {
                    code = payload.code;
                }
                _ => {}
            }
        }
        *state.child.lock().unwrap() = None;

        let is_cancelled = *state.cancelled.lock().unwrap();
        if is_cancelled {
            let _ = std::fs::remove_file(&dest);
            report.cancelled = true;
            break;
        }

        if code == Some(0) && dest.exists() {
            let size_after = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
            report.converted += 1;
            report.total_size_before += size_before;
            report.total_size_after += size_after;
            if report.first_output_dir.is_none() {
                report.first_output_dir = dest.parent().map(|p| p.to_string_lossy().to_string());
            }
            let _ = app.emit("video-progress", &VideoProgress {
                current: i + 1, total, file_name, file_percent: 100,
                size_before, size_after, status: "ok".into(), error_message: None,
            });
        } else {
            let _ = std::fs::remove_file(&dest);
            report.errors += 1;
            let msg = tail
                .iter()
                .rev()
                .find(|l| !l.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "ffmpeg failed".into());
            let _ = app.emit("video-progress", &VideoProgress {
                current: i + 1, total, file_name, file_percent: 0,
                size_before, size_after: 0, status: "error".into(),
                error_message: Some(msg),
            });
        }
    }

    let _ = app.emit("video-done", &report);
    Ok(report)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(VideoState::default())
        .invoke_handler(tauri::generate_handler![
            scan_folder,
            convert,
            convert_files,
            open_path,
            scan_videos,
            convert_videos,
            cancel_video
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 2: Verify it compiles and unit tests still pass**

Run: `cd src-tauri && cargo build && cargo test --lib`
Expected: builds; all unit tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "Wire video commands and ffmpeg sidecar orchestration"
```

---

## Task 9: Sidecar config — externalBin + capabilities

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: Add `externalBin` to `tauri.conf.json`**

In `src-tauri/tauri.conf.json`, change the `bundle` object so it begins like this (add the `externalBin` line right after `"targets": "all",`):

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "externalBin": ["binaries/ffmpeg"],
    "icon": [
```

(Leave the rest of the `bundle` object unchanged.)

- [ ] **Step 2: Grant the sidecar permission in `capabilities/default.json`**

Replace the full contents of `src-tauri/capabilities/default.json` with:

```json
{
  "identifier": "default",
  "description": "Default capabilities for WebP Converter",
  "windows": ["*"],
  "permissions": [
    "core:default",
    "dialog:default",
    "dialog:allow-open",
    {
      "identifier": "shell:allow-execute",
      "allow": [
        {
          "name": "binaries/ffmpeg",
          "sidecar": true,
          "args": true
        }
      ]
    }
  ]
}
```

- [ ] **Step 3: Verify JSON is valid (build still works)**

Run: `cd src-tauri && cargo build`
Expected: builds. (Full bundle validation happens in Task 14 once the binary is present.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json src-tauri/capabilities/default.json
git commit -m "Configure ffmpeg sidecar externalBin and shell capability"
```

---

## Task 10: FFmpeg fetch scripts + .gitignore

**Files:**
- Create: `scripts/fetch-ffmpeg.ps1`
- Create: `scripts/fetch-ffmpeg.sh`
- Modify: `.gitignore`

- [ ] **Step 1: Create `scripts/fetch-ffmpeg.ps1`**

```powershell
# Downloads a static GPL FFmpeg build (with libx264 + libvpx-vp9 + aac/opus)
# and places it as the Windows Tauri sidecar binary.
$ErrorActionPreference = "Stop"

$version = "7.1"
$url = "https://github.com/GyanD/codexffmpeg/releases/download/$version/ffmpeg-$version-essentials_build.zip"
$root = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $root "src-tauri/binaries"
$target = Join-Path $binDir "ffmpeg-x86_64-pc-windows-msvc.exe"

if (Test-Path $target) {
    Write-Host "FFmpeg already present: $target"
    exit 0
}

New-Item -ItemType Directory -Force -Path $binDir | Out-Null
$tmpZip = Join-Path $env:TEMP "ffmpeg-$version.zip"
$tmpDir = Join-Path $env:TEMP "ffmpeg-$version-extract"

Write-Host "Downloading FFmpeg $version (GPL essentials build)..."
Invoke-WebRequest -Uri $url -OutFile $tmpZip

Write-Host "Extracting..."
if (Test-Path $tmpDir) { Remove-Item -Recurse -Force $tmpDir }
Expand-Archive -Path $tmpZip -DestinationPath $tmpDir

$exe = Get-ChildItem -Path $tmpDir -Recurse -Filter "ffmpeg.exe" | Select-Object -First 1
if (-not $exe) { throw "ffmpeg.exe not found in archive" }
Copy-Item $exe.FullName $target -Force

Write-Host "FFmpeg ready: $target"
& $target -version | Select-Object -First 1
```

- [ ] **Step 2: Create `scripts/fetch-ffmpeg.sh`**

```bash
#!/usr/bin/env bash
# Downloads a static FFmpeg build and places it as the Tauri sidecar binary
# for the current platform's target triple.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"
mkdir -p "$BIN_DIR"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64) TRIPLE="x86_64-unknown-linux-gnu";;
      aarch64) TRIPLE="aarch64-unknown-linux-gnu";;
      *) echo "Unsupported arch: $ARCH"; exit 1;;
    esac
    URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-${ARCH}-static.tar.xz"
    TARGET="$BIN_DIR/ffmpeg-$TRIPLE"
    if [ -f "$TARGET" ]; then echo "FFmpeg already present: $TARGET"; exit 0; fi
    TMP="$(mktemp -d)"
    echo "Downloading $URL ..."
    curl -L "$URL" -o "$TMP/ffmpeg.tar.xz"
    tar -xJf "$TMP/ffmpeg.tar.xz" -C "$TMP"
    FF="$(find "$TMP" -type f -name ffmpeg | head -n1)"
    cp "$FF" "$TARGET"; chmod +x "$TARGET"
    ;;
  Darwin)
    case "$ARCH" in
      arm64) TRIPLE="aarch64-apple-darwin";;
      x86_64) TRIPLE="x86_64-apple-darwin";;
      *) echo "Unsupported arch: $ARCH"; exit 1;;
    esac
    URL="https://evermeet.cx/ffmpeg/getrelease/zip"
    TARGET="$BIN_DIR/ffmpeg-$TRIPLE"
    if [ -f "$TARGET" ]; then echo "FFmpeg already present: $TARGET"; exit 0; fi
    TMP="$(mktemp -d)"
    echo "Downloading $URL ..."
    curl -L "$URL" -o "$TMP/ffmpeg.zip"
    unzip -o "$TMP/ffmpeg.zip" -d "$TMP" >/dev/null
    cp "$TMP/ffmpeg" "$TARGET"; chmod +x "$TARGET"
    ;;
  *)
    echo "Unsupported OS: $OS"; exit 1;;
esac

echo "FFmpeg ready: $TARGET"
"$TARGET" -version | head -n1
```

- [ ] **Step 3: Append to `.gitignore`**

Add these lines to the end of `.gitignore` (the binaries are large and platform-specific; fetched at build time):

```
# Bundled FFmpeg sidecar binaries (fetched via scripts/fetch-ffmpeg.*)
src-tauri/binaries/
```

- [ ] **Step 4: Run the Windows fetch script to verify it works**

Run: `pwsh -File scripts/fetch-ffmpeg.ps1` (or `powershell -File scripts/fetch-ffmpeg.ps1`)
Expected: prints a final line like `ffmpeg version 7.1 ...`; file `src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe` exists.

- [ ] **Step 5: Commit**

```bash
git add scripts/fetch-ffmpeg.ps1 scripts/fetch-ffmpeg.sh .gitignore
git commit -m "Add ffmpeg fetch scripts and ignore bundled binaries"
```

---

## Task 11: Integration test (ffmpeg-gated)

**Files:**
- Create: `src-tauri/tests/video_integration.rs`

- [ ] **Step 1: Write the integration test**

```rust
// End-to-end FFmpeg encode test. Skips itself if no ffmpeg binary is available
// (bundled sidecar or on PATH), so it is safe to run in any environment.
use std::path::PathBuf;
use std::process::Command;

use webp_converter::video::{build_ffmpeg_args, VideoFormat};

fn find_ffmpeg() -> Option<PathBuf> {
    let triple_exe = if cfg!(windows) {
        "ffmpeg-x86_64-pc-windows-msvc.exe"
    } else if cfg!(target_os = "macos") {
        "ffmpeg-aarch64-apple-darwin"
    } else {
        "ffmpeg-x86_64-unknown-linux-gnu"
    };
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(triple_exe);
    if bundled.exists() {
        return Some(bundled);
    }
    let probe = if cfg!(windows) { "where" } else { "which" };
    if let Ok(out) = Command::new(probe).arg("ffmpeg").output() {
        if out.status.success() {
            if let Some(line) = String::from_utf8_lossy(&out.stdout).lines().next() {
                let p = line.trim();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
    }
    None
}

#[test]
fn ffmpeg_produces_web_mp4() {
    let Some(ff) = find_ffmpeg() else {
        eprintln!("ffmpeg not found — skipping integration test");
        return;
    };

    let tmp = std::env::temp_dir().join("wc_video_it");
    std::fs::create_dir_all(&tmp).unwrap();
    let src = tmp.join("src.mp4");

    let gen = Command::new(&ff)
        .args([
            "-y", "-f", "lavfi",
            "-i", "testsrc=duration=2:size=1280x720:rate=30",
            "-pix_fmt", "yuv420p",
            src.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run ffmpeg to generate source");
    assert!(gen.status.success(), "gen failed: {}", String::from_utf8_lossy(&gen.stderr));

    let out = tmp.join("web").join("src.mp4");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let args = build_ffmpeg_args(
        src.to_str().unwrap(),
        out.to_str().unwrap(),
        VideoFormat::Mp4,
        28,
        480,
        false,
    );
    let res = Command::new(&ff)
        .args(&args)
        .output()
        .expect("failed to run ffmpeg encode");
    assert!(res.status.success(), "encode failed: {}", String::from_utf8_lossy(&res.stderr));

    assert!(out.exists(), "output file was not produced");
    let after = std::fs::metadata(&out).unwrap().len();
    assert!(after > 0, "output file is empty");
    let before = std::fs::metadata(&src).unwrap().len();
    eprintln!("before={before} after={after}");
}
```

- [ ] **Step 2: Run the integration test**

Run: `cd src-tauri && cargo test --test video_integration -- --nocapture`
Expected: PASS. With the binary fetched in Task 10, it actually encodes and prints `before=... after=...`. If ffmpeg were missing it would print the skip line and still pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/video_integration.rs
git commit -m "Add ffmpeg-gated integration test for video encode"
```

---

## Task 12: Frontend — HTML (tabs + video panel + cancel)

**Files:**
- Modify: `src/index.html` (full replacement below)

- [ ] **Step 1: Replace `src/index.html` with the full new content**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>WebP Converter</title>
  <link rel="stylesheet" href="style.css" />
</head>
<body>
  <div class="app">
    <header>
      <h1>WebP Converter</h1>
      <p class="subtitle">Batch convert images &amp; videos for the web</p>
    </header>

    <div class="tabs">
      <button class="tab active" id="tab-image" data-tab="image">Images (WebP)</button>
      <button class="tab" id="tab-video" data-tab="video">Vidéos (Web)</button>
    </div>

    <main>
      <!-- Selection (shared) -->
      <div class="section" id="folder-section">
        <div class="folder-row">
          <button id="btn-folder" class="btn btn-primary">Select folder</button>
          <button id="btn-files" class="btn btn-primary">Select files</button>
          <span id="folder-path" class="folder-path">No selection</span>
        </div>
        <label class="checkbox-label" id="recursive-row">
          <input type="checkbox" id="recursive" checked />
          Include subfolders
        </label>
        <div id="scan-info" class="scan-info hidden"></div>
      </div>

      <!-- Image settings -->
      <div class="section settings-panel" id="image-settings">
        <div class="settings-grid">
          <div class="setting">
            <label for="quality">Quality</label>
            <div class="slider-row">
              <input type="range" id="quality" min="1" max="100" value="80" />
              <span id="quality-value" class="setting-value">80</span>
            </div>
          </div>
          <div class="setting">
            <label for="max-width">Max width (px)</label>
            <input type="number" id="max-width" value="1920" min="100" max="10000" />
          </div>
        </div>
      </div>

      <!-- Video settings -->
      <div class="section settings-panel hidden" id="video-settings">
        <div class="settings-grid">
          <div class="setting">
            <label for="video-format">Format</label>
            <select id="video-format">
              <option value="mp4" selected>MP4 (H.264) — compatible partout</option>
              <option value="webm">WebM (VP9) — plus léger</option>
            </select>
          </div>
          <div class="setting">
            <label for="video-quality">Quality</label>
            <div class="slider-row">
              <input type="range" id="video-quality" min="1" max="100" value="80" />
              <span id="video-quality-value" class="setting-value">80</span>
            </div>
          </div>
          <div class="setting">
            <label for="max-height">Max height (px)</label>
            <input type="number" id="max-height" value="1080" min="120" max="4320" />
          </div>
          <div class="setting">
            <label class="checkbox-label">
              <input type="checkbox" id="silent" />
              Silencieux (retirer l'audio)
            </label>
          </div>
        </div>
        <p class="hint">WebM/VP9 : ~30 % plus léger, mais encodage plus lent que MP4.</p>
      </div>

      <!-- Convert -->
      <div class="section center">
        <button id="btn-convert" class="btn btn-convert" disabled>Convert to WebP</button>
      </div>

      <!-- Progress -->
      <div id="progress-section" class="section hidden">
        <div class="progress-bar-container">
          <div id="progress-bar" class="progress-bar"></div>
        </div>
        <div id="progress-text" class="progress-text"></div>
        <div id="progress-file" class="progress-file"></div>
        <div class="center">
          <button id="btn-cancel" class="btn btn-cancel hidden">Annuler</button>
        </div>
      </div>

      <!-- Report -->
      <div id="report-section" class="section hidden">
        <h2>Done!</h2>
        <div class="report-grid">
          <div class="report-item">
            <span class="report-label">Converted</span>
            <span id="report-converted" class="report-value ok">0</span>
          </div>
          <div class="report-item">
            <span class="report-label">Skipped</span>
            <span id="report-skipped" class="report-value skip">0</span>
          </div>
          <div class="report-item">
            <span class="report-label">Errors</span>
            <span id="report-errors" class="report-value err">0</span>
          </div>
          <div class="report-item wide">
            <span class="report-label">Size</span>
            <span id="report-size" class="report-value"></span>
          </div>
        </div>
        <div class="center" style="margin-top: 1rem;">
          <button id="btn-open-folder" class="btn btn-primary hidden">Open output folder</button>
        </div>
      </div>
    </main>

    <footer>
      <a href="https://github.com/magipa-consulting/webp-converter" target="_blank">GitHub</a>
      <span>·</span>
      <span>MIT License</span>
      <span>·</span>
      <span>MAGIPA Consulting</span>
    </footer>
  </div>

  <script src="main.js"></script>
</body>
</html>
```

- [ ] **Step 2: Commit**

```bash
git add src/index.html
git commit -m "Add tab toggle and video settings panel to UI"
```

---

## Task 13: Frontend — CSS (tabs, select, hint, cancel)

**Files:**
- Modify: `src/style.css` (append a block)

- [ ] **Step 1: Append to `src/style.css`**

```css
/* Tabs */
.tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 24px;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 4px;
}

.tab {
  flex: 1;
  border: none;
  background: transparent;
  color: var(--text-dim);
  font-size: 14px;
  font-weight: 600;
  padding: 8px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.tab:hover:not(.active):not(:disabled) {
  color: var(--text);
}

.tab.active {
  background: var(--accent);
  color: #fff;
}

.tab:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* Select dropdown */
select {
  background: var(--bg);
  border: 1px solid var(--border);
  color: var(--text);
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 14px;
  width: 100%;
  outline: none;
  cursor: pointer;
}

select:focus {
  border-color: var(--accent);
}

/* Hint text */
.hint {
  font-size: 12px;
  color: var(--text-dim);
  margin-top: 10px;
}

/* Recursive row spacing */
#recursive-row {
  margin-top: 12px;
}

/* Cancel button */
.btn-cancel {
  background: transparent;
  color: var(--red);
  border: 1px solid var(--border);
  margin-top: 12px;
}

.btn-cancel:hover:not(:disabled) {
  border-color: var(--red);
}

.btn-cancel:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
```

- [ ] **Step 2: Commit**

```bash
git add src/style.css
git commit -m "Style tabs, select, hint and cancel button"
```

---

## Task 14: Frontend — JS (tab switching, video scan/convert/cancel, events)

**Files:**
- Modify: `src/main.js` (full replacement below)

- [ ] **Step 1: Replace `src/main.js` with the full new content**

```javascript
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open } = window.__TAURI__.dialog;

const IMAGE_EXTS = ["jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif"];
const VIDEO_EXTS = ["mp4", "mov", "avi", "mkv", "webm", "m4v", "flv", "wmv", "mpg", "mpeg", "m2ts", "3gp"];

// DOM
const tabImage = document.getElementById("tab-image");
const tabVideo = document.getElementById("tab-video");
const imageSettings = document.getElementById("image-settings");
const videoSettings = document.getElementById("video-settings");
const btnFolder = document.getElementById("btn-folder");
const btnFiles = document.getElementById("btn-files");
const folderPath = document.getElementById("folder-path");
const recursiveRow = document.getElementById("recursive-row");
const recursive = document.getElementById("recursive");
const scanInfo = document.getElementById("scan-info");
const quality = document.getElementById("quality");
const qualityValue = document.getElementById("quality-value");
const maxWidth = document.getElementById("max-width");
const videoFormat = document.getElementById("video-format");
const videoQuality = document.getElementById("video-quality");
const videoQualityValue = document.getElementById("video-quality-value");
const maxHeight = document.getElementById("max-height");
const silent = document.getElementById("silent");
const btnConvert = document.getElementById("btn-convert");
const progressSection = document.getElementById("progress-section");
const progressBar = document.getElementById("progress-bar");
const progressText = document.getElementById("progress-text");
const progressFile = document.getElementById("progress-file");
const btnCancel = document.getElementById("btn-cancel");
const reportSection = document.getElementById("report-section");
const btnOpenFolder = document.getElementById("btn-open-folder");

let currentTab = "image";
let selectedFolder = null;
let selectedFiles = null; // array of file paths
let mode = null; // "folder" | "files"

function humanSize(bytes) {
  if (bytes >= 1073741824) return (bytes / 1073741824).toFixed(1) + " GB";
  if (bytes >= 1048576) return (bytes / 1048576).toFixed(1) + " MB";
  if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
  return bytes + " B";
}

function resetSelection() {
  selectedFolder = null;
  selectedFiles = null;
  mode = null;
  folderPath.textContent = "No selection";
  folderPath.classList.remove("active");
  scanInfo.classList.add("hidden");
  reportSection.classList.add("hidden");
  progressSection.classList.add("hidden");
  btnConvert.disabled = true;
}

// Tabs
function setTab(tab) {
  currentTab = tab;
  const isImage = tab === "image";
  tabImage.classList.toggle("active", isImage);
  tabVideo.classList.toggle("active", !isImage);
  imageSettings.classList.toggle("hidden", !isImage);
  videoSettings.classList.toggle("hidden", isImage);
  btnConvert.textContent = isImage ? "Convert to WebP" : "Convert video";
  resetSelection();
}
tabImage.addEventListener("click", () => setTab("image"));
tabVideo.addEventListener("click", () => setTab("video"));

// Quality sliders
quality.addEventListener("input", () => {
  qualityValue.textContent = quality.value;
});
videoQuality.addEventListener("input", () => {
  videoQualityValue.textContent = videoQuality.value;
});

function currentExts() {
  return currentTab === "image" ? IMAGE_EXTS : VIDEO_EXTS;
}

async function scanCurrentFolder() {
  if (currentTab === "image") {
    return await invoke("scan_folder", { folder: selectedFolder, recursive: recursive.checked });
  }
  return await invoke("scan_videos", {
    folder: selectedFolder,
    recursive: recursive.checked,
    format: videoFormat.value,
  });
}

function showScan(scan) {
  if (scan.files.length === 0) {
    scanInfo.textContent = "Nothing new to convert (all done or empty).";
    scanInfo.classList.remove("hidden");
    btnConvert.disabled = true;
  } else {
    const p = scan.files.length > 1 ? "s" : "";
    const fp = scan.folder_count > 1 ? "s" : "";
    const noun = currentTab === "image" ? "image" : "video";
    scanInfo.textContent = `${scan.files.length} ${noun}${p} to convert (${humanSize(scan.total_size)}) in ${scan.folder_count} folder${fp}`;
    scanInfo.classList.remove("hidden");
    btnConvert.disabled = false;
  }
  reportSection.classList.add("hidden");
  progressSection.classList.add("hidden");
}

// Folder selection
btnFolder.addEventListener("click", async () => {
  const folder = await open({ directory: true, multiple: false });
  if (!folder) return;

  mode = "folder";
  selectedFolder = folder;
  selectedFiles = null;
  folderPath.textContent = folder;
  folderPath.classList.add("active");
  recursiveRow.classList.remove("hidden");

  const scan = await scanCurrentFolder();
  selectedFiles = scan.files; // resolved list (used by video convert)
  showScan(scan);
});

// File selection
btnFiles.addEventListener("click", async () => {
  const files = await open({
    directory: false,
    multiple: true,
    filters: [{ name: currentTab === "image" ? "Images" : "Videos", extensions: currentExts() }],
  });
  if (!files || files.length === 0) return;

  mode = "files";
  selectedFiles = Array.isArray(files) ? files : [files];
  selectedFolder = null;
  recursiveRow.classList.add("hidden");

  const p = selectedFiles.length > 1 ? "s" : "";
  const noun = currentTab === "image" ? "image" : "video";
  folderPath.textContent = `${selectedFiles.length} ${noun}${p} selected`;
  folderPath.classList.add("active");
  scanInfo.textContent = selectedFiles.map((f) => f.split("\\").pop().split("/").pop()).join(", ");
  scanInfo.classList.remove("hidden");
  btnConvert.disabled = false;

  reportSection.classList.add("hidden");
  progressSection.classList.add("hidden");
});

// Re-scan when recursive changes (folder mode)
recursive.addEventListener("change", async () => {
  if (mode !== "folder" || !selectedFolder) return;
  const scan = await scanCurrentFolder();
  selectedFiles = scan.files;
  showScan(scan);
});

// Re-scan when video format changes (folder mode, video tab)
videoFormat.addEventListener("change", async () => {
  if (currentTab !== "video" || mode !== "folder" || !selectedFolder) return;
  const scan = await scanCurrentFolder();
  selectedFiles = scan.files;
  showScan(scan);
});

// Convert
btnConvert.addEventListener("click", async () => {
  if (!mode) return;

  btnConvert.disabled = true;
  btnFolder.disabled = true;
  btnFiles.disabled = true;
  tabImage.disabled = true;
  tabVideo.disabled = true;
  btnOpenFolder.classList.add("hidden");
  reportSection.classList.add("hidden");
  progressSection.classList.remove("hidden");
  progressBar.style.width = "0%";
  progressText.textContent = "Starting...";
  progressFile.textContent = "";

  let report;
  if (currentTab === "image") {
    btnCancel.classList.add("hidden");
    if (mode === "folder") {
      report = await invoke("convert", {
        folder: selectedFolder,
        recursive: recursive.checked,
        quality: parseFloat(quality.value),
        maxWidth: parseInt(maxWidth.value),
      });
    } else {
      report = await invoke("convert_files", {
        files: selectedFiles,
        quality: parseFloat(quality.value),
        maxWidth: parseInt(maxWidth.value),
      });
    }
  } else {
    btnCancel.classList.remove("hidden");
    btnCancel.disabled = false;
    btnCancel.textContent = "Annuler";
    report = await invoke("convert_videos", {
      files: selectedFiles,
      format: videoFormat.value,
      quality: parseInt(videoQuality.value),
      maxHeight: parseInt(maxHeight.value),
      silent: silent.checked,
    });
  }

  showReport(report);
  btnFolder.disabled = false;
  btnFiles.disabled = false;
  tabImage.disabled = false;
  tabVideo.disabled = false;
  btnCancel.classList.add("hidden");
});

// Cancel (video only)
btnCancel.addEventListener("click", async () => {
  btnCancel.disabled = true;
  btnCancel.textContent = "Annulation...";
  try {
    await invoke("cancel_video");
  } catch (e) {
    // ignore — convert_videos will resolve with cancelled report
  }
});

// Image progress
listen("convert-progress", (event) => {
  const p = event.payload;
  const pct = Math.round((p.current / p.total) * 100);
  progressBar.style.width = pct + "%";
  progressText.textContent = `${p.current} / ${p.total} (${pct}%)`;

  if (p.status === "ok") {
    const gain = p.size_before > 0 ? Math.round((1 - p.size_after / p.size_before) * 100) : 0;
    progressFile.textContent = `${p.file_name} — ${humanSize(p.size_before)} → ${humanSize(p.size_after)} (−${gain}%)`;
  } else if (p.status === "error") {
    progressFile.textContent = `${p.file_name} — error: ${p.error_message || "unknown"}`;
  }
});

// Video progress
listen("video-progress", (event) => {
  const p = event.payload;
  const overall = Math.min(100, Math.round((((p.current - 1) + p.file_percent / 100) / p.total) * 100));
  progressBar.style.width = overall + "%";

  if (p.status === "encoding") {
    progressText.textContent = `Fichier ${p.current} / ${p.total} — ${p.file_percent}% (global ${overall}%)`;
    progressFile.textContent = p.file_name;
  } else if (p.status === "ok") {
    progressText.textContent = `Fichier ${p.current} / ${p.total} — terminé`;
    const gain = p.size_before > 0 ? Math.round((1 - p.size_after / p.size_before) * 100) : 0;
    progressFile.textContent = `${p.file_name} — ${humanSize(p.size_before)} → ${humanSize(p.size_after)} (−${gain}%)`;
  } else if (p.status === "skip") {
    progressFile.textContent = `${p.file_name} — déjà converti`;
  } else if (p.status === "error") {
    progressFile.textContent = `${p.file_name} — erreur: ${p.error_message || "unknown"}`;
  }
});

listen("video-done", () => {
  btnCancel.disabled = false;
  btnCancel.textContent = "Annuler";
});

function showReport(report) {
  progressSection.classList.add("hidden");
  reportSection.classList.remove("hidden");

  document.getElementById("report-converted").textContent = report.converted;
  document.getElementById("report-skipped").textContent = report.skipped;
  document.getElementById("report-errors").textContent = report.errors;

  if (report.converted > 0) {
    const gain = report.total_size_before - report.total_size_after;
    const gainPct = report.total_size_before > 0 ? Math.round((gain / report.total_size_before) * 100) : 0;
    document.getElementById("report-size").textContent =
      `${humanSize(report.total_size_before)} → ${humanSize(report.total_size_after)} (−${humanSize(gain)}, −${gainPct}%)`;
  } else {
    document.getElementById("report-size").textContent = "—";
  }

  if (report.first_output_dir) {
    btnOpenFolder.classList.remove("hidden");
    btnOpenFolder.onclick = () => invoke("open_path", { path: report.first_output_dir });
  } else {
    btnOpenFolder.classList.add("hidden");
  }

  const tail = report.cancelled ? " (annulé)" : "";
  scanInfo.textContent = (currentTab === "image" ? "All images converted." : "Conversion vidéo terminée.") + tail;
  btnConvert.disabled = true;
}

setTab("image");
```

- [ ] **Step 2: Commit**

```bash
git add src/main.js
git commit -m "Add tab switching and video scan/convert/cancel flow"
```

---

## Task 15: End-to-end app run + manual QA

**Files:** none (verification only)

- [ ] **Step 1: Ensure the ffmpeg binary is present**

Run: `pwsh -File scripts/fetch-ffmpeg.ps1`
Expected: `src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe` exists.

- [ ] **Step 2: Launch the app in dev**

Run: `npm install` (first time only), then `npx tauri dev`
Expected: app window opens, "Images (WebP)" and "Vidéos (Web)" tabs visible.

- [ ] **Step 3: Image regression check**

In the Images tab: select a folder with a few JPG/PNG, convert. Expected: works exactly as before (a `webp/` folder is created, size report shows a reduction).

- [ ] **Step 4: Video happy path**

Switch to Vidéos tab. Select a folder (or files) containing a real `.mov`/`.mp4`. Keep MP4, quality 80, 1080. Convert. Expected: progress bar advances with per-file %, a `web/<name>.mp4` is produced, report shows a size reduction, "Open output folder" opens the `web/` folder.

- [ ] **Step 5: WebM path**

Switch Format to WebM, convert another video. Expected: a `web/<name>.webm` is produced (encoding is noticeably slower).

- [ ] **Step 6: Cancel + silent + skip checks**

- Start a long video and click Annuler mid-encode → report shows `(annulé)`, no partial file remains in `web/`.
- Tick "Silencieux", convert → output has no audio track.
- Re-run a folder already converted → videos are skipped.

- [ ] **Step 7: Full unit + integration test sweep**

Run: `cd src-tauri && cargo test`
Expected: all unit tests and the integration test pass.

- [ ] **Step 8: Commit (if any fixes were needed)**

```bash
git add -A
git commit -m "Fixes from manual QA of video conversion"
```

(If no fixes were needed, skip this commit.)

---

## Task 16: Docs — README reposition + FFmpeg notice

**Files:**
- Modify: `README.md`
- Create: `NOTICE.md`

- [ ] **Step 1: Update `README.md`**

Replace the intro block (lines under the title down to the screenshot) so the size/dependency claim is accurate:

```markdown
# WebP Converter

Convert images to WebP and heavy videos to web-light MP4/WebM in one click. Drop a folder, hit convert, done.

**No command line. The image converter stays tiny; video conversion bundles FFmpeg.**

![WebP Converter screenshot](screenshot.png)
```

Replace the "What it does" section with:

```markdown
## What it does

**Images** — converts JPG, PNG, GIF, BMP, TIFF to WebP, resizes to a max width (default 1920px, no upscale), writes to a `webp/` folder next to the originals.

**Videos** — converts MP4, MOV, AVI, MKV, WebM and more to web-light **MP4 (H.264)** or **WebM (VP9)** using constant-quality (CRF) encoding — visually lossless, much smaller. Caps height (default 1080p), optional audio removal, writes to a `web/` folder next to the originals. Originals are never touched.
```

Replace the "Settings" table section with:

```markdown
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
```

Replace the "Build from source" section with:

```markdown
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
```

- [ ] **Step 2: Create `NOTICE.md`**

```markdown
# Third-Party Notices

## FFmpeg

WebP Converter bundles a static build of **FFmpeg** to perform video conversion.

The bundled FFmpeg build includes `libx264` and `libvpx`, which makes that
binary licensed under the **GNU General Public License, version 2 or later
(GPLv2+)**. FFmpeg is a separate program invoked as an external binary; it is
aggregated with, not linked into, WebP Converter.

- FFmpeg project: https://ffmpeg.org
- FFmpeg source and license: https://ffmpeg.org/download.html
- The bundled Windows build is sourced from https://www.gyan.dev/ffmpeg/builds/

WebP Converter's own source code remains under the MIT License (see `LICENSE`).
The GPL applies to the bundled FFmpeg binary only.
```

- [ ] **Step 3: Commit**

```bash
git add README.md NOTICE.md
git commit -m "Document video conversion, FFmpeg bundling and GPL notice"
```

---

## Self-Review (completed during planning)

**Spec coverage check:**
- Sidecar integration (spec 4.1) → Tasks 1, 8, 9, 10.
- `video.rs` pure core + scan (spec 4.2) → Tasks 2-7.
- CRF mapping + ffmpeg templates (spec 4.3) → Tasks 4, 6 (+ integration test 11).
- Progress parsing (spec 4.4) → Task 5 + orchestration in Task 8.
- Cancellation (spec 4.5) → Task 8 (`cancel_video`, state) + Task 14 (UI).
- New commands (spec 4.6) → Task 8.
- UI mirror + tabs (spec 4.7) → Tasks 12, 13, 14.
- Error handling (spec 7) → Task 8 per-file error branches.
- Testing (spec 8) → Tasks 2-7 unit + Task 11 integration + Task 15 manual.
- Build/CI fetch (spec 9) → Task 10.
- Licensing + README (spec 10) → Task 16.

**Type consistency check:** `VideoFormat`, `VideoProgress`, `VideoReport`, `VideoScanResult` defined once in Task 1 and used identically in Tasks 7, 8, 11. Function names (`quality_to_crf`, `build_ffmpeg_args`, `parse_duration`, `parse_out_time`, `output_path`, `is_video_file`, `scan_videos`) are stable across tasks. Event names (`video-progress`, `video-done`) and command names (`scan_videos`, `convert_videos`, `cancel_video`) match between Rust (Task 8) and JS (Task 14). camelCase JS args (`maxHeight`) map to Rust snake_case (`max_height`) per Tauri's default convention.

**Placeholder scan:** No TBD/TODO; every code step contains full content.

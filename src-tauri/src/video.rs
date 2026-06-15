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

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn output_path(src: &Path, format: VideoFormat) -> Option<PathBuf> {
    let parent = src.parent()?;
    let stem = src.file_stem()?;
    let out_dir = parent.join(VIDEO_OUTPUT_DIR);
    Some(out_dir.join(format!("{}.{}", stem.to_string_lossy(), format.extension())))
}

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
}

mod converter;
pub mod video;

use std::path::Path;
use std::sync::Mutex;

use converter::{ConvertReport, ScanResult};
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
        // Cancel may have arrived while we were preparing this file — kill the
        // freshly spawned child instead of letting it run to completion.
        let is_cancelled = *state.cancelled.lock().unwrap();
        if is_cancelled {
            let _ = child.kill();
            report.cancelled = true;
            break;
        }
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

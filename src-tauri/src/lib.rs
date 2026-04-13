mod converter;

use std::path::Path;

use converter::{ConvertProgress, ConvertReport, ScanResult};
use tauri::{AppHandle, Emitter};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![scan_folder, convert, convert_files, open_path])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

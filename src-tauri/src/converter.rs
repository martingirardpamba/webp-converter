use image::imageops::FilterType;
use image::GenericImageView;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif"];
const OUTPUT_DIR: &str = "webp";

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub files: Vec<String>,
    pub total_size: u64,
    pub folder_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConvertProgress {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
    pub size_before: u64,
    pub size_after: u64,
    pub status: String, // "ok", "skip", "error"
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConvertReport {
    pub converted: usize,
    pub skipped: usize,
    pub errors: usize,
    pub total_size_before: u64,
    pub total_size_after: u64,
}

fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn scan_folder(folder: &str, recursive: bool) -> ScanResult {
    let mut files = Vec::new();
    let mut total_size: u64 = 0;
    let mut folders_seen = std::collections::HashSet::new();

    if recursive {
        for entry in WalkDir::new(folder)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            // Skip output directories
            if path.components().any(|c| c.as_os_str() == OUTPUT_DIR) {
                continue;
            }
            if path.is_file() && is_image_file(path) {
                if let Some(parent) = path.parent() {
                    let out_dir = parent.join(OUTPUT_DIR);
                    let stem = path.file_stem().unwrap_or_default();
                    let dest = out_dir.join(format!("{}.webp", stem.to_string_lossy()));
                    // Only include files not yet converted
                    if !dest.exists() {
                        if let Ok(meta) = fs::metadata(path) {
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
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && is_image_file(&path) {
                    let out_dir = dir.join(OUTPUT_DIR);
                    let stem = path.file_stem().unwrap_or_default();
                    let dest = out_dir.join(format!("{}.webp", stem.to_string_lossy()));
                    if !dest.exists() {
                        if let Ok(meta) = fs::metadata(&path) {
                            total_size += meta.len();
                        }
                        folders_seen.insert(dir.to_path_buf());
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    ScanResult {
        files,
        total_size,
        folder_count: folders_seen.len(),
    }
}

pub fn convert_image(
    src_path: &str,
    quality: f32,
    max_width: u32,
) -> ConvertProgress {
    let path = Path::new(src_path);
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let parent = match path.parent() {
        Some(p) => p,
        None => {
            return ConvertProgress {
                current: 0,
                total: 0,
                file_name,
                size_before: 0,
                size_after: 0,
                status: "error".to_string(),
                error_message: Some("Invalid path".to_string()),
            };
        }
    };

    let out_dir = parent.join(OUTPUT_DIR);
    let stem = path.file_stem().unwrap_or_default();
    let dest = out_dir.join(format!("{}.webp", stem.to_string_lossy()));

    // Skip if already exists
    if dest.exists() {
        return ConvertProgress {
            current: 0,
            total: 0,
            file_name,
            size_before: 0,
            size_after: 0,
            status: "skip".to_string(),
            error_message: None,
        };
    }

    let size_before = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    // Create output directory
    if let Err(e) = fs::create_dir_all(&out_dir) {
        return ConvertProgress {
            current: 0,
            total: 0,
            file_name,
            size_before,
            size_after: 0,
            status: "error".to_string(),
            error_message: Some(format!("Cannot create output dir: {}", e)),
        };
    }

    // Open and decode image
    let img = match image::open(path) {
        Ok(img) => img,
        Err(e) => {
            return ConvertProgress {
                current: 0,
                total: 0,
                file_name,
                size_before,
                size_after: 0,
                status: "error".to_string(),
                error_message: Some(format!("Cannot open image: {}", e)),
            };
        }
    };

    // Resize if wider than max_width (preserve aspect ratio, no upscale)
    let (w, h) = img.dimensions();
    let img = if w > max_width {
        let new_h = (h as f64 * max_width as f64 / w as f64) as u32;
        img.resize(max_width, new_h, FilterType::Lanczos3)
    } else {
        img
    };

    // Encode to WebP
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), w, h);
    let webp_data = encoder.encode(quality);

    // Write file
    if let Err(e) = fs::write(&dest, &*webp_data) {
        return ConvertProgress {
            current: 0,
            total: 0,
            file_name,
            size_before,
            size_after: 0,
            status: "error".to_string(),
            error_message: Some(format!("Cannot write WebP: {}", e)),
        };
    }

    let size_after = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);

    ConvertProgress {
        current: 0,
        total: 0,
        file_name,
        size_before,
        size_after,
        status: "ok".to_string(),
        error_message: None,
    }
}

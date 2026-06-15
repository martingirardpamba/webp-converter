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

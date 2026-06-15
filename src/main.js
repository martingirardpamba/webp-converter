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

  try {
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
  } catch (e) {
    progressSection.classList.add("hidden");
    scanInfo.classList.remove("hidden");
    scanInfo.textContent = "Erreur: " + (e && e.message ? e.message : e);
  } finally {
    btnFolder.disabled = false;
    btnFiles.disabled = false;
    tabImage.disabled = false;
    tabVideo.disabled = false;
    btnCancel.classList.add("hidden");
  }
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

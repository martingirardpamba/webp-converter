const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open } = window.__TAURI__.dialog;

// DOM elements
const btnFolder = document.getElementById("btn-folder");
const btnFiles = document.getElementById("btn-files");
const folderPath = document.getElementById("folder-path");
const scanInfo = document.getElementById("scan-info");
const quality = document.getElementById("quality");
const qualityValue = document.getElementById("quality-value");
const maxWidth = document.getElementById("max-width");
const recursive = document.getElementById("recursive");
const btnConvert = document.getElementById("btn-convert");
const progressSection = document.getElementById("progress-section");
const progressBar = document.getElementById("progress-bar");
const progressText = document.getElementById("progress-text");
const progressFile = document.getElementById("progress-file");
const reportSection = document.getElementById("report-section");
const btnOpenFolder = document.getElementById("btn-open-folder");

let selectedFolder = null;
let selectedFiles = null; // array of file paths
let mode = null; // "folder" or "files"

function humanSize(bytes) {
  if (bytes >= 1073741824) return (bytes / 1073741824).toFixed(1) + " GB";
  if (bytes >= 1048576) return (bytes / 1048576).toFixed(1) + " MB";
  if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
  return bytes + " B";
}

// Quality slider
quality.addEventListener("input", () => {
  qualityValue.textContent = quality.value;
});

// Folder selection
btnFolder.addEventListener("click", async () => {
  const folder = await open({ directory: true, multiple: false });
  if (!folder) return;

  mode = "folder";
  selectedFolder = folder;
  selectedFiles = null;
  folderPath.textContent = folder;
  folderPath.classList.add("active");
  recursive.parentElement.classList.remove("hidden");

  // Scan
  const scan = await invoke("scan_folder", {
    folder,
    recursive: recursive.checked,
  });

  if (scan.files.length === 0) {
    scanInfo.textContent = "No new images to convert (all already done or folder empty).";
    scanInfo.classList.remove("hidden");
    btnConvert.disabled = true;
  } else {
    const plural = scan.files.length > 1 ? "s" : "";
    const folderPlural = scan.folder_count > 1 ? "s" : "";
    scanInfo.textContent = `${scan.files.length} image${plural} to convert (${humanSize(scan.total_size)}) in ${scan.folder_count} folder${folderPlural}`;
    scanInfo.classList.remove("hidden");
    btnConvert.disabled = false;
  }

  reportSection.classList.add("hidden");
  progressSection.classList.add("hidden");
});

// File selection
btnFiles.addEventListener("click", async () => {
  const files = await open({
    directory: false,
    multiple: true,
    filters: [{
      name: "Images",
      extensions: ["jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif"]
    }]
  });
  if (!files || files.length === 0) return;

  mode = "files";
  selectedFiles = Array.isArray(files) ? files : [files];
  selectedFolder = null;
  recursive.parentElement.classList.add("hidden");

  const plural = selectedFiles.length > 1 ? "s" : "";
  folderPath.textContent = `${selectedFiles.length} image${plural} selected`;
  folderPath.classList.add("active");
  scanInfo.textContent = selectedFiles.map(f => f.split("\\").pop().split("/").pop()).join(", ");
  scanInfo.classList.remove("hidden");
  btnConvert.disabled = false;

  reportSection.classList.add("hidden");
  progressSection.classList.add("hidden");
});

// Re-scan when recursive changes
recursive.addEventListener("change", async () => {
  if (!selectedFolder) return;
  const scan = await invoke("scan_folder", {
    folder: selectedFolder,
    recursive: recursive.checked,
  });

  if (scan.files.length === 0) {
    scanInfo.textContent = "No new images to convert.";
    btnConvert.disabled = true;
  } else {
    const plural = scan.files.length > 1 ? "s" : "";
    scanInfo.textContent = `${scan.files.length} image${plural} to convert (${humanSize(scan.total_size)})`;
    btnConvert.disabled = false;
  }
});

// Convert
btnConvert.addEventListener("click", async () => {
  if (!mode) return;

  btnConvert.disabled = true;
  btnFolder.disabled = true;
  btnFiles.disabled = true;
  btnOpenFolder.classList.add("hidden");
  reportSection.classList.add("hidden");
  progressSection.classList.remove("hidden");
  progressBar.style.width = "0%";
  progressText.textContent = "Starting...";
  progressFile.textContent = "";

  let report;
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

  showReport(report);
  btnFolder.disabled = false;
  btnFiles.disabled = false;
});

// Progress events
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

function showReport(report) {
  progressSection.classList.add("hidden");
  reportSection.classList.remove("hidden");

  document.getElementById("report-converted").textContent = report.converted;
  document.getElementById("report-skipped").textContent = report.skipped;
  document.getElementById("report-errors").textContent = report.errors;

  if (report.converted > 0) {
    const gain = report.total_size_before - report.total_size_after;
    const gainPct = report.total_size_before > 0
      ? Math.round((gain / report.total_size_before) * 100)
      : 0;
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

  scanInfo.textContent = "All images converted.";
  btnConvert.disabled = true;
}

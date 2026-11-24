//! The probe slice's Tauri commands.
//!
//! Both read PATH afresh on each call rather than caching the startup result: the search costs
//! microseconds, and keeping no state means `probe_source` always runs the ffprobe that
//! `find_ffmpeg` would report.

use std::path::PathBuf;

use crate::probe::domain::{FfmpegTools, ProbeError, SourceInfo};
use crate::probe::infrastructure::{ffprobe, tools};
use crate::search_path::SearchPath;

/// Reports where `ffmpeg` and `ffprobe` are and their versions, or which is missing or broken.
/// The frontend calls it at startup and shows the setup message unless both are found.
#[tauri::command]
pub async fn find_ffmpeg() -> FfmpegTools {
    // Spawning two processes blocks; keep it off the async runtime's worker threads.
    tauri::async_runtime::spawn_blocking(|| tools::find_tools(&SearchPath::from_env()))
        .await
        .expect("find_ffmpeg task panicked")
}

/// Probes the source at `path`, which must be an absolute path to an existing file.
#[tauri::command]
pub async fn probe_source(path: String) -> Result<SourceInfo, ProbeError> {
    // Trust boundary: the path comes from the frontend. Requiring an absolute path to a file
    // also keeps it from reaching ffprobe as an option (`-...`) or a protocol URL (`http:...`).
    let file = PathBuf::from(&path);
    if !file.is_absolute() || !file.is_file() {
        return Err(ProbeError::NotFound { path });
    }
    tauri::async_runtime::spawn_blocking(move || {
        let ffprobe_path = SearchPath::from_env()
            .locate_tools()
            .ffprobe
            .ok_or(ProbeError::FfprobeMissing)?;
        ffprobe::probe(&ffprobe_path, &file)
    })
    .await
    .expect("probe_source task panicked")
}

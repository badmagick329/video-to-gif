//! Running the ffmpeg that writes the trim timeline's thumbnails.

use std::fs;
use std::path::{Path, PathBuf};

use crate::preview::domain::{PreviewError, thumbnail_args};
use crate::process;

/// Writes the thumbnails of `input` (`duration` seconds) into `dir`, which must exist and be
/// empty, and returns their paths in time order. On failure `dir` is left for the caller to
/// delete.
pub fn make(
    ffmpeg: &Path,
    input: &Path,
    duration: f64,
    dir: &Path,
) -> Result<Vec<PathBuf>, PreviewError> {
    let failed = |reason: String| PreviewError::ThumbnailsFailed { reason };
    let output = process::command(ffmpeg)
        .args(thumbnail_args(input, duration, dir))
        .output()
        .map_err(|err| failed(format!("ffmpeg could not start: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("ffmpeg exited with {}", output.status));
        return Err(failed(reason));
    }
    let mut thumbnails: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|err| failed(err.to_string()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    // `thumb-%02d.jpg` sorts by name in time order.
    thumbnails.sort();
    if thumbnails.is_empty() {
        return Err(failed("ffmpeg wrote no frames".to_owned()));
    }
    Ok(thumbnails)
}

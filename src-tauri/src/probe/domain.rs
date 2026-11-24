//! What the probe slice knows, as plain types: whether ffmpeg and ffprobe can run, and a
//! source's validated metadata. These cross IPC as they are (serde), so the TypeScript mirror in
//! `src/probe.ts` must change with them.

use std::fmt;

use serde::Serialize;

/// One of the two programs the app runs, as found (or not) on PATH.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Tool {
    /// Resolved on PATH and answered `-version` like the real program.
    Found { path: String, version: String },
    /// No file of that name in any PATH directory.
    Missing,
    /// A file of that name exists but did not run as ffmpeg/ffprobe, so the app cannot use it.
    Unusable { path: String, reason: String },
}

/// Result of the startup check. The app's main view needs both to be [`Tool::Found`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FfmpegTools {
    pub ffmpeg: Tool,
    pub ffprobe: Tool,
}

/// A source's metadata, validated from ffprobe's output.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    /// Seconds, finite and positive.
    pub duration: f64,
    /// Displayed size in pixels: the coded size after rotation metadata is applied. ffmpeg
    /// auto-rotates before any filter and the webview shows the rotated picture, so crop
    /// rectangles and output sizes are in these pixels.
    pub width: u32,
    pub height: u32,
    /// Frames per second, finite and positive.
    pub frame_rate: f64,
    /// ffprobe's `codec_name` for the video stream, e.g. `h264`, `hevc`, `av1`, `vp9`.
    pub video_codec: String,
    /// ffprobe's `format_name`, a comma-separated list of demuxer names, e.g.
    /// `mov,mp4,m4a,3gp,3g2,mj2` or `matroska,webm`.
    pub container: String,
    pub has_audio: bool,
}

/// Why a source could not be probed. Serialized with a `kind` tag so the frontend can show
/// each case in its own words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProbeError {
    /// ffprobe is not on PATH (the setup message should already have said so).
    FfprobeMissing,
    /// ffprobe was found but could not be started.
    FfprobeFailed { reason: String },
    /// The path is not an absolute path to an existing file.
    NotFound { path: String },
    /// ffprobe could not read the file: not a media file, corrupt, or access denied.
    Unreadable { path: String, reason: String },
    /// The file has no video stream other than cover art, e.g. an audio file.
    NoVideoStream { path: String },
    /// ffprobe's output was missing a field the app needs or held an impossible value.
    InvalidOutput { path: String, reason: String },
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::FfprobeMissing => write!(f, "ffprobe was not found on PATH"),
            ProbeError::FfprobeFailed { reason } => write!(f, "ffprobe could not start: {reason}"),
            ProbeError::NotFound { path } => write!(f, "no file at {path}"),
            ProbeError::Unreadable { path, reason } => write!(f, "cannot read {path}: {reason}"),
            ProbeError::NoVideoStream { path } => write!(f, "{path} has no video stream"),
            ProbeError::InvalidOutput { path, reason } => {
                write!(f, "unexpected ffprobe output for {path}: {reason}")
            }
        }
    }
}

impl std::error::Error for ProbeError {}

/// The size the picture is displayed at, given its coded size and rotation in degrees (either
/// sign, any multiple of 90). A quarter turn swaps width and height.
///
/// Rotations off a multiple of 90 are rounded to the nearest quarter turn, matching ffmpeg's
/// autorotate, which only applies transpose/flip for quarter turns.
pub fn displayed_size(coded_width: u32, coded_height: u32, rotation_degrees: f64) -> (u32, u32) {
    let quarter_turns = (rotation_degrees / 90.0).round() as i64;
    if quarter_turns.rem_euclid(2) == 1 {
        (coded_height, coded_width)
    } else {
        (coded_width, coded_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarter_turns_swap_width_and_height() {
        for rotation in [90.0, -90.0, 270.0, -270.0, 89.6] {
            assert_eq!(
                displayed_size(1920, 1080, rotation),
                (1080, 1920),
                "{rotation}"
            );
        }
    }

    #[test]
    fn half_and_no_turns_keep_the_coded_size() {
        for rotation in [0.0, 180.0, -180.0, 360.0] {
            assert_eq!(
                displayed_size(1920, 1080, rotation),
                (1920, 1080),
                "{rotation}"
            );
        }
    }
}

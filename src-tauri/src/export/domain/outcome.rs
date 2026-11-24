//! What an export reports back over IPC: progress, each output, and why a run could not start.
//! These cross IPC as they are (serde); `src/ipc/export.ts` mirrors them, so change both together.

use std::fmt;

use serde::Serialize;

use crate::export::domain::settings::FileType;

/// How far the output being made has got, emitted as the `export-progress` event.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub file_type: FileType,
    /// From 0 to 1: ffmpeg's output time over the output's expected seconds. A GIF, a reversed
    /// output and Boomerang hold their frames until the input ends, so they sit near 0 and then
    /// jump.
    pub progress: f64,
}

/// One ticked file type's result, emitted as the `export-output` event when it is settled and
/// returned, in run order, when the export ends.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportOutput {
    /// Written to `path` (the name actually used, numbered if the plain one was taken).
    Done {
        file_type: FileType,
        path: String,
        /// Bytes.
        size: u64,
        /// The encoded frame size in pixels, the same for every output of a run.
        width: u32,
        height: u32,
    },
    /// ffmpeg failed, or the finished file could not be put in place. `reason` is the tail of
    /// ffmpeg's error output, or the filesystem's error. Nothing was left in the folder.
    Failed { file_type: FileType, reason: String },
    /// Stopped by `cancel_export`, or never started because of it. Nothing was left in the folder.
    Cancelled { file_type: FileType },
}

/// Why an export could not start. Once it starts, each file type's problems are reported in its
/// [`ExportOutput`] instead. Serialized with a `kind` tag so the frontend can word each case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportError {
    /// The source is not an absolute path to an existing file.
    NotFound { path: String },
    /// The destination is not an absolute path to an existing folder.
    DestinationNotFound { path: String },
    /// Something in the request is impossible: bad source info, a selection or crop outside the
    /// source, no file types, a caption image of the wrong size.
    InvalidRequest { reason: String },
    /// ffmpeg is not on PATH.
    FfmpegMissing,
    /// Reverse, Boomerang or a long GIF would hold more frames in memory than the machine has free.
    /// Both figures in bytes.
    NotEnoughMemory {
        held_bytes: u64,
        available_bytes: u64,
    },
    /// Another export is running; one runs at a time.
    Busy,
    /// The caption image could not be written to the temp folder.
    CaptionsFailed { reason: String },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::NotFound { path } => write!(f, "no file at {path}"),
            ExportError::DestinationNotFound { path } => write!(f, "no folder at {path}"),
            ExportError::InvalidRequest { reason } => write!(f, "invalid export: {reason}"),
            ExportError::FfmpegMissing => write!(f, "ffmpeg was not found on PATH"),
            ExportError::NotEnoughMemory {
                held_bytes,
                available_bytes,
            } => write!(
                f,
                "needs {held_bytes} bytes of frames in memory, {available_bytes} are free"
            ),
            ExportError::Busy => write!(f, "another export is running"),
            ExportError::CaptionsFailed { reason } => {
                write!(f, "could not write the caption image: {reason}")
            }
        }
    }
}

impl std::error::Error for ExportError {}

/// Why Open, Show in folder or Copy refused a path. Serialized with a `kind` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OutputError {
    /// Not an output this session made. Open, Show in folder and Copy act only on those, so the
    /// webview cannot ask the shell to open, or the clipboard to hand out, an arbitrary path.
    NotAnOutput { path: String },
    /// This session made it, but it has since been moved or deleted.
    Missing { path: String },
    /// The shell refused to open or show it.
    ShellFailed { reason: String },
    /// The clipboard could not be opened or written, usually because another program held it.
    ClipboardFailed { reason: String },
}

/// Why a folder could not be remembered as the destination. Serialized with a `kind` tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DestinationError {
    /// Not an absolute path to an existing folder.
    NotFound { path: String },
    /// The choice could not be written to the app's config folder.
    NotSaved { reason: String },
}

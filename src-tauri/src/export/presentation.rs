//! The export slice's Tauri commands, its `export-progress` and `export-output` events, and its
//! part in the app's startup and exit.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager};

use crate::export::application::{self, Environment, Exports};
use crate::export::domain::memory::MemoryEstimate;
use crate::export::domain::outcome::{ExportError, ExportOutput};
use crate::export::domain::plan::{ExportRequest, MemoryRequest};
use crate::export::infrastructure::memory;
use crate::search_path::SearchPath;
use crate::temp_folder::{self, TempFolder};

/// Event carrying [`crate::export::domain::outcome::ExportProgress`] while an output is made.
pub const EXPORT_PROGRESS_EVENT: &str = "export-progress";
/// Event carrying each [`ExportOutput`] as it settles, before `run_export` resolves with all of
/// them.
pub const EXPORT_OUTPUT_EVENT: &str = "export-output";

/// Puts the export state in Tauri's state. Called from the app's setup.
pub fn set_up(app: &tauri::App) {
    app.manage(Exports::default());
}

/// Cancels a running export and waits for it to delete its partial file. Called when the app
/// exits, before the preview slice deletes the temp folder.
pub fn shut_down(app: &AppHandle) {
    if let Some(exports) = app.try_state::<Exports>() {
        exports.shut_down();
    }
}

/// Exports the source in `request` into the folder `destination`, one output per ticked file type
/// in the order GIF, WebP, MP4, emitting `export-progress` while each is made and `export-output`
/// as each settles. Resolves with every output once the last has settled; a failed or cancelled
/// file type is an output with that status, not an error. Rejects with an [`ExportError`] when the
/// export cannot start: a bad request, ffmpeg missing, not enough free memory for the frames
/// Reverse, Boomerang or a GIF hold, or another export running.
#[tauri::command]
pub async fn run_export(
    app: AppHandle,
    request: ExportRequest,
    destination: String,
) -> Result<Vec<ExportOutput>, ExportError> {
    // Trust boundary: the request comes from the frontend. Requiring an absolute path to a file
    // also keeps the source from reaching ffmpeg as an option (`-...`) or a protocol URL.
    let source = PathBuf::from(&request.path);
    if !source.is_absolute() || !source.is_file() {
        return Err(ExportError::NotFound { path: request.path });
    }
    let destination_dir = PathBuf::from(&destination);
    if !destination_dir.is_absolute() || !destination_dir.is_dir() {
        return Err(ExportError::DestinationNotFound { path: destination });
    }
    let plan = request.validate(destination_dir)?;

    // ffmpeg runs for as long as the export takes; keep it off the async runtime's workers.
    tauri::async_runtime::spawn_blocking(move || {
        let ffmpeg = SearchPath::from_env()
            .locate_tools()
            .ffmpeg
            .ok_or(ExportError::FfmpegMissing)?;
        let work_dir = temp_folder::session_dir(&TempFolder::default_root());
        let exports = app.state::<Exports>();
        let running = exports.begin()?;
        let env = Environment {
            ffmpeg: &ffmpeg,
            work_dir: &work_dir,
            available_memory: memory::available_bytes(),
        };
        // A failed emit only costs the UI an update; the resolved value still carries every
        // output.
        application::run(
            &plan,
            env,
            running.control(),
            |progress| {
                let _ = app.emit(EXPORT_PROGRESS_EVENT, progress);
            },
            |output| {
                let _ = app.emit(EXPORT_OUTPUT_EVENT, output);
            },
        )
    })
    .await
    .expect("run_export task panicked")
}

/// Cancels the running export, if any: its ffmpeg is killed, its partial file deleted, and its
/// remaining file types reported as cancelled. Returns at once.
#[tauri::command]
pub fn cancel_export(app: AppHandle) {
    app.state::<Exports>().cancel();
}

/// How much memory the frames of an export with these settings would hold, against the
/// machine's free memory now, so the UI can warn before Convert. `run_export` refuses a run that
/// does not fit.
#[tauri::command]
pub fn estimate_export_memory(request: MemoryRequest) -> Result<MemoryEstimate, ExportError> {
    let selection = request.validate()?;
    Ok(MemoryEstimate::new(&selection, memory::available_bytes()))
}

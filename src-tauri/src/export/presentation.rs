//! The export slice's Tauri commands, its `export-progress` and `export-output` events, and its
//! part in the app's startup and exit.
//!
//! Each finished output is added to the asset protocol's scope as it is made, so the session list
//! can draw its thumbnail, and to [`SessionOutputs`], so Open, Show in folder and Copy accept it.
//! Nothing else the export touches becomes readable.

use std::error::Error;
use std::path::{Path, PathBuf};

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::export::application::estimate::{self, Estimates};
use crate::export::application::session_outputs::SessionOutputs;
use crate::export::application::{self, Environment, Exports};
use crate::export::domain::estimate::SizeEstimate;
use crate::export::domain::memory::MemoryEstimate;
use crate::export::domain::outcome::{DestinationError, ExportError, ExportOutput, OutputError};
use crate::export::domain::plan::{ExportRequest, MemoryRequest};
use crate::export::infrastructure::clipboard;
use crate::export::infrastructure::destination::{self, SavedDestination};
use crate::export::infrastructure::memory;
use crate::search_path::SearchPath;
use crate::temp_folder::{self, TempFolder};

/// Event carrying [`crate::export::domain::outcome::ExportProgress`] while an output is made.
pub const EXPORT_PROGRESS_EVENT: &str = "export-progress";
/// Event carrying each [`ExportOutput`] as it settles, before `run_export` resolves with all of
/// them.
pub const EXPORT_OUTPUT_EVENT: &str = "export-output";

/// Puts the export and estimate state, the session's outputs and the remembered destination in
/// Tauri's state. Called from the app's setup.
pub fn set_up(app: &tauri::App) -> Result<(), Box<dyn Error>> {
    app.manage(Exports::default());
    app.manage(Estimates::default());
    app.manage(SessionOutputs::default());
    app.manage(SavedDestination::in_dir(&app.path().app_config_dir()?));
    Ok(())
}

/// Cancels a running export and waits for it to delete its partial file, and stops a running
/// size estimate. Called when the app exits, before the preview slice deletes the temp folder.
pub fn shut_down(app: &AppHandle) {
    if let Some(estimates) = app.try_state::<Estimates>() {
        estimates.cancel(true);
    }
    if let Some(exports) = app.try_state::<Exports>() {
        exports.shut_down();
    }
}

/// Estimates the size of each ticked file type in `request` (the request `run_export` takes) by
/// encoding samples of it in the background, sending each [`SizeEstimate`] on `on_estimate` as it
/// settles. Resolves with all of them. Starting an estimate cancels the running one, whose file
/// types settle as cancelled. Rejects with an [`ExportError`] for a bad request, ffmpeg missing,
/// or an export running (`busy`): estimates pause while one does.
#[tauri::command]
pub async fn estimate_output_size(
    app: AppHandle,
    request: ExportRequest,
    on_estimate: Channel<SizeEstimate>,
) -> Result<Vec<SizeEstimate>, ExportError> {
    // Trust boundary, as for run_export.
    let source = PathBuf::from(&request.path);
    if !source.is_absolute() || !source.is_file() {
        return Err(ExportError::NotFound { path: request.path });
    }
    let work_dir = temp_folder::session_dir(&TempFolder::default_root());
    // The samples go to the temp folder; the plan's destination is never written to.
    let plan = request.validate(work_dir.clone())?;
    if app.state::<Exports>().is_running() {
        return Err(ExportError::Busy);
    }
    let control = app.state::<Estimates>().begin();

    tauri::async_runtime::spawn_blocking(move || {
        let ffmpeg = SearchPath::from_env()
            .locate_tools()
            .ffmpeg
            .ok_or(ExportError::FfmpegMissing)?;
        // Optional: only a gifski GIF needs it, and that file type fails without it.
        let gifski = SearchPath::from_env().find("gifski");
        let env = Environment {
            ffmpeg: &ffmpeg,
            gifski: gifski.as_deref(),
            work_dir: &work_dir,
            available_memory: memory::available_bytes(),
        };
        // A failed send only costs the UI an update; the resolved value still carries them all.
        Ok(estimate::estimate(&plan, env, &control, &|size| {
            let _ = on_estimate.send(size.clone());
        }))
    })
    .await
    .expect("estimate task panicked")
}

/// Stops the running size estimate, if any. Returns at once. The editor calls it on every change,
/// while the video plays, and before an export.
#[tauri::command]
pub fn cancel_size_estimate(app: AppHandle) {
    app.state::<Estimates>().cancel(false);
}

/// Exports the source in `request` into the folder `destination`, one output per ticked file type
/// in the order GIF, WebP, MP4, emitting `export-progress` while each is made and `export-output`
/// as each settles. Resolves with every output once the last has settled; a failed or cancelled
/// file type is an output with that status, not an error. Rejects with an [`ExportError`] when the
/// export cannot start: a bad request, ffmpeg missing, not enough free memory for the frames
/// Reverse, Boomerang or a GIF hold (unless `request.acceptPaging`), or another export running.
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
        // Optional: only a gifski GIF needs it, and that file type fails without it.
        let gifski = SearchPath::from_env().find("gifski");
        let work_dir = temp_folder::session_dir(&TempFolder::default_root());
        let exports = app.state::<Exports>();
        let running = exports.begin()?;
        // Estimates pause while an export runs: they would slow it, and its settings are final.
        app.state::<Estimates>().cancel(false);
        let env = Environment {
            ffmpeg: &ffmpeg,
            gifski: gifski.as_deref(),
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
                if let ExportOutput::Done { path, .. } = output {
                    let path = Path::new(path);
                    app.state::<SessionOutputs>().record(path);
                    // Without it the list shows a broken thumbnail; Open still works.
                    let _ = app.asset_protocol_scope().allow_file(path);
                }
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
/// does not fit unless the request accepts paging.
#[tauri::command]
pub fn estimate_export_memory(request: MemoryRequest) -> Result<MemoryEstimate, ExportError> {
    let selection = request.validate()?;
    Ok(MemoryEstimate::new(&selection, memory::available_bytes()))
}

/// The folder chosen with Save to on this or an earlier launch, when it still exists; `None`
/// means the editor saves beside the source.
#[tauri::command]
pub fn saved_destination(saved: State<'_, SavedDestination>) -> Option<String> {
    saved.load().map(|folder| folder.display().to_string())
}

/// Remembers `folder` as the destination for this and later launches, or forgets the choice with
/// `None` so outputs go beside the source again.
#[tauri::command]
pub fn remember_destination(
    saved: State<'_, SavedDestination>,
    folder: Option<String>,
) -> Result<(), DestinationError> {
    // Trust boundary: the folder comes from the frontend; hold it to what run_export accepts.
    let folder = folder.map(PathBuf::from);
    if let Some(folder) = &folder
        && !destination::is_folder(folder)
    {
        return Err(DestinationError::NotFound {
            path: folder.display().to_string(),
        });
    }
    saved
        .save(folder.as_deref())
        .map_err(|err| DestinationError::NotSaved {
            reason: err.to_string(),
        })
}

/// Opens an output this session made in the app Windows associates with its type.
#[tauri::command]
pub async fn open_output(
    outputs: State<'_, SessionOutputs>,
    path: String,
) -> Result<(), OutputError> {
    let path = outputs.check(&path)?;
    shell(move || tauri_plugin_opener::open_path(path, None::<&str>)).await
}

/// Opens the folder holding an output this session made, in Explorer, with the file selected.
#[tauri::command]
pub async fn show_output_in_folder(
    outputs: State<'_, SessionOutputs>,
    path: String,
) -> Result<(), OutputError> {
    let path = outputs.check(&path)?;
    shell(move || tauri_plugin_opener::reveal_item_in_dir(path)).await
}

/// Puts an output this session made on the clipboard as a file (`CF_HDROP`), so it pastes into an
/// Explorer folder as a copy and into chat apps as an attachment.
#[tauri::command]
pub async fn copy_output(
    outputs: State<'_, SessionOutputs>,
    path: String,
) -> Result<(), OutputError> {
    // The checked path is the string sent, byte for byte; the clipboard gets that string.
    outputs.check(&path)?;
    // Opening a clipboard another program holds is retried with short sleeps.
    tauri::async_runtime::spawn_blocking(move || clipboard::copy_files(&[&path]))
        .await
        .expect("clipboard task panicked")
        .map_err(|reason| OutputError::ClipboardFailed { reason })
}

/// Runs a shell call off the async runtime's workers: it initialises COM and may wait on Explorer.
async fn shell(
    call: impl FnOnce() -> Result<(), tauri_plugin_opener::Error> + Send + 'static,
) -> Result<(), OutputError> {
    tauri::async_runtime::spawn_blocking(call)
        .await
        .expect("shell task panicked")
        .map_err(|err| OutputError::ShellFailed {
            reason: err.to_string(),
        })
}

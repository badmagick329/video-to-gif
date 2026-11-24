//! The preview slice's Tauri commands, its `proxy-progress` event, and its part in the app's
//! startup and exit.
//!
//! The webview reads the source, proxies and thumbnails through Tauri's asset protocol. Its scope starts
//! empty (`tauri.conf.json`); this module widens it at runtime to this session's temp folder and
//! to each source as it is opened, so the webview can read nothing else on disk.

use std::error::Error;
use std::path::PathBuf;

use tauri::{App, AppHandle, Emitter, Manager, State};

use crate::preview::domain::{PreviewError, PreviewSource, ProxyProgress};
use crate::preview::infrastructure::session::Previewer;
use crate::preview::infrastructure::temp_folder::TempFolder;
use crate::probe::domain::SourceInfo;
use crate::search_path::SearchPath;

/// Event carrying [`ProxyProgress`] while a proxy builds.
pub const PROXY_PROGRESS_EVENT: &str = "proxy-progress";

/// Sweeps the temp folder of what a crashed session left, creates this session's folder, lets
/// the webview read it and its subfolders, and puts the [`Previewer`] in Tauri's state. Called from the app's setup.
pub fn set_up(app: &App) -> Result<(), Box<dyn Error>> {
    let previewer = Previewer::new(TempFolder::create(&TempFolder::default_root())?);
    // Recursive, for the thumbnail folders inside it; the app writes everything in there.
    app.asset_protocol_scope()
        .allow_directory(previewer.proxy_dir(), true)?;
    app.manage(previewer);
    Ok(())
}

/// Stops a running proxy build and deletes the temp folder. Called when the app exits.
pub fn shut_down(app: &AppHandle) {
    if let Some(previewer) = app.try_state::<Previewer>() {
        // Nothing to report to at exit; the next startup's sweep retries what is left.
        let _ = previewer.shut_down();
    }
}

/// Opens the source at `path`, described by `info` from `probe_source`, and says whether the
/// player can play it as it is or needs a proxy from `make_proxy`. Replacing an open source
/// cancels its proxy build and deletes its proxy.
#[tauri::command]
pub async fn open_source(
    app: AppHandle,
    previewer: State<'_, Previewer>,
    path: String,
    info: SourceInfo,
) -> Result<PreviewSource, PreviewError> {
    // Trust boundary: both arguments come from the frontend.
    let file = PathBuf::from(&path);
    if !file.is_absolute() || !file.is_file() {
        return Err(PreviewError::NotFound { path });
    }
    check_info(&info)?;
    app.asset_protocol_scope()
        .allow_file(&file)
        .map_err(|err| PreviewError::NotServable {
            path: path.clone(),
            reason: err.to_string(),
        })?;
    Ok(previewer.open(file, info))
}

/// Closes the source at `path` when it is the open source, as the editor removes its clip: stops a
/// running proxy build and deletes its proxy and thumbnails. Any other path is left alone, since
/// a newer open has already replaced it. The webview keeps read access to the file until exit,
/// because Tauri's asset scope cannot shrink.
#[tauri::command]
pub async fn close_source(app: AppHandle, path: String) {
    // Waits for a killed ffmpeg to exit; keep that off the async runtime's workers.
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Previewer>().close(&PathBuf::from(path));
    })
    .await
    .expect("close_source task panicked");
}

/// Returns the path of the open source's proxy, building it first if needed and emitting
/// `proxy-progress` events meanwhile. `path` must be the open source.
#[tauri::command]
pub async fn make_proxy(app: AppHandle, path: String) -> Result<String, PreviewError> {
    // Encoding blocks for as long as ffmpeg runs; keep it off the async runtime's workers.
    tauri::async_runtime::spawn_blocking(move || {
        let previewer = app.state::<Previewer>();
        let source = PathBuf::from(&path);
        let ffmpeg = || SearchPath::from_env().locate_tools().ffmpeg;
        let report = |progress: f64| {
            let event = ProxyProgress {
                source: path.clone(),
                progress,
            };
            // A failed emit only costs a progress update.
            let _ = app.emit(PROXY_PROGRESS_EVENT, event);
        };
        previewer
            .proxy(&source, ffmpeg, report)
            .map(|proxy| proxy.display().to_string())
    })
    .await
    .expect("make_proxy task panicked")
}

/// Returns the paths of the open source's timeline thumbnails, in time order, making them from the
/// preview source first if needed. `path` must be the open source. The files live in this
/// session's temp folder, which the webview may read.
#[tauri::command]
pub async fn make_thumbnails(app: AppHandle, path: String) -> Result<Vec<String>, PreviewError> {
    // ffmpeg runs for seconds; keep it off the async runtime's workers.
    tauri::async_runtime::spawn_blocking(move || {
        let previewer = app.state::<Previewer>();
        let ffmpeg = || SearchPath::from_env().locate_tools().ffmpeg;
        previewer
            .thumbnails(&PathBuf::from(&path), ffmpeg)
            .map(|files| {
                files
                    .iter()
                    .map(|file| file.display().to_string())
                    .collect()
            })
    })
    .await
    .expect("make_thumbnails task panicked")
}

/// Rejects source info no probe could have produced; the frontend sends back what
/// `probe_source` returned, but the command cannot assume so.
fn check_info(info: &SourceInfo) -> Result<(), PreviewError> {
    let reason = if !(info.duration.is_finite() && info.duration > 0.0) {
        "duration must be positive"
    } else if info.width == 0 || info.height == 0 {
        "size must be positive"
    } else if !(info.frame_rate.is_finite() && info.frame_rate > 0.0) {
        "frame rate must be positive"
    } else {
        return Ok(());
    };
    Err(PreviewError::InvalidSourceInfo {
        reason: reason.to_owned(),
    })
}

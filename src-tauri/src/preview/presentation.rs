//! The preview slice's Tauri commands, its `proxy-progress` event, and its part in the app's
//! startup and exit.
//!
//! The webview reads the source, proxies and thumbnails through Tauri's asset protocol. Its scope starts
//! empty (`tauri.conf.json`); this module widens it at runtime to this session's temp folder and
//! to each source as it is opened, so the webview can read nothing else on disk.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{App, AppHandle, Emitter, Manager, State};

use crate::preview::domain::{PreviewError, PreviewSource, ProxyProgress, RecentSource, TimeRange};
use crate::preview::infrastructure::recent::RecentSources;
use crate::preview::infrastructure::session::Previewer;
use crate::probe::domain::SourceInfo;
use crate::search_path::SearchPath;
use crate::temp_folder::TempFolder;

/// Event carrying [`ProxyProgress`] while a proxy builds.
pub const PROXY_PROGRESS_EVENT: &str = "proxy-progress";

/// Sweeps the temp folder of what a crashed session left, creates this session's folder, lets
/// the webview read it and its subfolders, and puts the [`Previewer`] and [`RecentSources`] in
/// Tauri's state. Called from the app's setup.
pub fn set_up(app: &App) -> Result<(), Box<dyn Error>> {
    let previewer = Previewer::new(TempFolder::create(&TempFolder::default_root())?);
    // Recursive, for the thumbnail folders inside it; the app writes everything in there.
    app.asset_protocol_scope()
        .allow_directory(previewer.proxy_dir(), true)?;
    app.manage(previewer);
    app.manage(RecentSources::in_dir(&app.path().app_config_dir()?));
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
/// cancels its proxy build and deletes its proxy. Every way of opening a source (the dialog, a
/// drop, Recent) ends here, so this is the only place the webview gains access to a source, and
/// the only place a source joins Recent.
#[tauri::command]
pub async fn open_source(
    app: AppHandle,
    previewer: State<'_, Previewer>,
    recent: State<'_, RecentSources>,
    path: String,
    info: SourceInfo,
) -> Result<PreviewSource, PreviewError> {
    open(&previewer, &recent, path, info, now_ms(), |file| {
        app.asset_protocol_scope()
            .allow_file(file)
            .map_err(|err| err.to_string())
    })
}

/// Recent, newest first, without sources whose files no longer exist (which it forgets). Recent
/// that cannot be read or written back is reported as empty: it is a convenience, and the
/// editor opens files without it.
#[tauri::command]
pub async fn recent_sources(recent: State<'_, RecentSources>) -> Result<Vec<RecentSource>, String> {
    Ok(recent.load().unwrap_or_default())
}

/// Takes `path` off Recent. The file is untouched.
#[tauri::command]
pub async fn forget_recent_source(
    recent: State<'_, RecentSources>,
    path: String,
) -> Result<(), String> {
    recent
        .remove(&PathBuf::from(path))
        .map_err(|err| err.to_string())
}

/// `open_source` without Tauri: `admit` gives the webview read access to the file.
fn open(
    previewer: &Previewer,
    recent: &RecentSources,
    path: String,
    info: SourceInfo,
    opened_at: u64,
    admit: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PreviewSource, PreviewError> {
    // Trust boundary: both arguments come from the frontend.
    let file = PathBuf::from(&path);
    if !file.is_absolute() || !file.is_file() {
        return Err(PreviewError::NotFound { path });
    }
    check_info(&info)?;
    admit(&file).map_err(|reason| PreviewError::NotServable {
        path: path.clone(),
        reason,
    })?;
    // Failing to remember the source costs only its Recent entry; the open itself worked.
    let _ = recent.add(RecentSource {
        path: file.clone(),
        duration: info.duration,
        opened_at,
    });
    Ok(previewer.open(file, info))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
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
/// preview source first if needed: the whole source's strip, or with `range` (`{ start, end }` in
/// seconds) a strip of just that span, which a zoomed timeline asks for. `path` must be the open
/// source. The files live in this session's temp folder, which the webview may read.
#[tauri::command]
pub async fn make_thumbnails(
    app: AppHandle,
    path: String,
    range: Option<TimeRange>,
) -> Result<Vec<String>, PreviewError> {
    // ffmpeg runs for seconds; keep it off the async runtime's workers.
    tauri::async_runtime::spawn_blocking(move || {
        let previewer = app.state::<Previewer>();
        let ffmpeg = || SearchPath::from_env().locate_tools().ffmpeg;
        previewer
            .thumbnails(&PathBuf::from(&path), range, ffmpeg)
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

#[cfg(test)]
mod tests;

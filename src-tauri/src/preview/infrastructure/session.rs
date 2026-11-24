//! The open source and its proxy, for the app's whole run.
//!
//! One source is open at a time, with at most one proxy, kept in the session's temp folder and
//! reused until another source replaces it or it is closed, and at most one folder of timeline
//! thumbnails. Replacing or closing the source cancels a proxy build still running and deletes the
//! old proxy and thumbnails; shutting down does the same and deletes the temp folder.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::preview::domain::{PreviewError, PreviewSource, choose_preview_source, proxy_args};
use crate::preview::infrastructure::proxy::ProxyJob;
use crate::preview::infrastructure::temp_folder::TempFolder;
use crate::preview::infrastructure::thumbnails;
use crate::probe::domain::SourceInfo;

/// The preview slice's state, managed by Tauri for the app's lifetime.
#[derive(Debug)]
pub struct Previewer {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    folder: TempFolder,
    current: Option<Current>,
    /// Numbers proxy files, so a new proxy never reuses the name of one still being deleted.
    next_proxy: u64,
    /// Numbers thumbnail folders, for the same reason.
    next_thumbnails: u64,
    shut_down: bool,
}

#[derive(Debug)]
struct Current {
    source: PathBuf,
    info: SourceInfo,
    proxy: Proxy,
    /// The finished thumbnails, kept so a second request (React's development double effects)
    /// costs nothing.
    thumbnails: Option<Thumbnails>,
}

#[derive(Debug)]
struct Thumbnails {
    dir: PathBuf,
    files: Vec<PathBuf>,
}

#[derive(Debug)]
enum Proxy {
    None,
    Building(Arc<Build>),
    Ready(PathBuf),
}

/// A proxy encode in flight. Callers that ask for the same proxy while it builds wait on `done`.
#[derive(Debug)]
struct Build {
    job: ProxyJob,
    result: Mutex<Option<Result<PathBuf, PreviewError>>>,
    done: Condvar,
}

impl Build {
    fn wait(&self) -> Result<PathBuf, PreviewError> {
        let mut result = lock(&self.result);
        loop {
            if let Some(result) = result.as_ref() {
                return result.clone();
            }
            result = self
                .done
                .wait(result)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    fn complete(&self, result: Result<PathBuf, PreviewError>) {
        *lock(&self.result) = Some(result);
        self.done.notify_all();
    }
}

impl Previewer {
    pub fn new(folder: TempFolder) -> Self {
        Self {
            state: Mutex::new(State {
                folder,
                current: None,
                next_proxy: 1,
                next_thumbnails: 1,
                shut_down: false,
            }),
        }
    }

    /// The session folder, where proxies and thumbnail folders are written.
    pub fn proxy_dir(&self) -> PathBuf {
        lock(&self.state).folder.session_dir().to_owned()
    }

    /// Makes `source` the open source and says what the player should play for it. Opening a
    /// different source cancels the previous one's proxy build and deletes its proxy; opening the
    /// same source again keeps its proxy.
    pub fn open(&self, source: PathBuf, info: SourceInfo) -> PreviewSource {
        let choice = choose_preview_source(&info, &source);
        let mut state = lock(&self.state);
        match state.current.as_mut() {
            Some(current) if current.source == source => current.info = info,
            _ => {
                if let Some(previous) = state.current.take() {
                    release(previous, false);
                }
                state.current = Some(Current {
                    source,
                    info,
                    proxy: Proxy::None,
                    thumbnails: None,
                });
            }
        }
        choice
    }

    /// Closes `source` when it is the open source: stops its proxy build, waiting for ffmpeg to
    /// exit, and deletes its proxy and thumbnails. A build's partial file is deleted by the thread
    /// running [`Self::proxy`], which then returns [`PreviewError::Cancelled`]. Closing a source that
    /// is not open does nothing, so a close that raced a newer open cannot close the newer source.
    pub fn close(&self, source: &Path) {
        let mut state = lock(&self.state);
        if state.current.as_ref().is_some_and(|c| c.source == source) {
            let current = state.current.take().expect("checked above");
            release(current, true);
        }
    }

    /// The open source's proxy: the existing one, the one already building, or a new build with
    /// the ffmpeg that `ffmpeg` resolves. Blocks until the proxy is ready, calling `on_progress`
    /// with 0..=1 while a new build runs. Fails with [`PreviewError::NotOpen`] unless `source` is
    /// the open source, and with [`PreviewError::Cancelled`] if another source replaces it first.
    pub fn proxy(
        &self,
        source: &Path,
        ffmpeg: impl FnOnce() -> Option<PathBuf>,
        on_progress: impl FnMut(f64),
    ) -> Result<PathBuf, PreviewError> {
        let mut state = lock(&self.state);
        if state.shut_down {
            return Err(PreviewError::Cancelled);
        }
        let not_open = || PreviewError::NotOpen {
            path: source.display().to_string(),
        };
        let current = state
            .current
            .as_ref()
            .filter(|c| c.source == source)
            .ok_or_else(not_open)?;
        match &current.proxy {
            Proxy::Ready(path) => return Ok(path.clone()),
            Proxy::Building(build) => {
                let build = Arc::clone(build);
                drop(state);
                return build.wait();
            }
            Proxy::None => {}
        }
        let info = current.info.clone();

        let ffmpeg = ffmpeg().ok_or(PreviewError::FfmpegMissing)?;
        let target = state
            .folder
            .session_dir()
            .join(format!("proxy-{}.mp4", state.next_proxy));
        state.next_proxy += 1;
        let args = proxy_args(source, &info, &target);
        let (job, output) = ProxyJob::start(&ffmpeg, &args)?;
        let build = Arc::new(Build {
            job,
            result: Mutex::new(None),
            done: Condvar::new(),
        });
        let current = state.current.as_mut().expect("checked above");
        current.proxy = Proxy::Building(Arc::clone(&build));
        drop(state);

        let mut result = build
            .job
            .finish(output, info.duration, &target, on_progress)
            .map(|()| target.clone());

        let mut state = lock(&self.state);
        let still_current = state.current.as_mut().filter(
            |current| matches!(&current.proxy, Proxy::Building(b) if Arc::ptr_eq(b, &build)),
        );
        match still_current {
            Some(current) => {
                current.proxy = match &result {
                    Ok(path) => Proxy::Ready(path.clone()),
                    Err(_) => Proxy::None,
                };
            }
            // Replaced between ffmpeg finishing and here: nobody will play this proxy.
            None => {
                if result.is_ok() {
                    let _ = std::fs::remove_file(&target);
                }
                result = Err(PreviewError::Cancelled);
            }
        }
        drop(state);
        build.complete(result.clone());
        result
    }

    /// The open source's timeline thumbnails, in time order: the existing ones, or new ones made
    /// with the ffmpeg that `ffmpeg` resolves from the preview source, the proxy once it is built
    /// and the source otherwise. Blocks while ffmpeg runs. Fails with [`PreviewError::NotOpen`]
    /// unless `source` is the open source, and with [`PreviewError::Cancelled`] if another source
    /// replaces it first.
    pub fn thumbnails(
        &self,
        source: &Path,
        ffmpeg: impl FnOnce() -> Option<PathBuf>,
    ) -> Result<Vec<PathBuf>, PreviewError> {
        let mut state = lock(&self.state);
        if state.shut_down {
            return Err(PreviewError::Cancelled);
        }
        let current = state
            .current
            .as_ref()
            .filter(|c| c.source == source)
            .ok_or_else(|| PreviewError::NotOpen {
                path: source.display().to_string(),
            })?;
        if let Some(thumbnails) = &current.thumbnails {
            return Ok(thumbnails.files.clone());
        }
        let input = match &current.proxy {
            Proxy::Ready(proxy) => proxy.clone(),
            _ => source.to_owned(),
        };
        let duration = current.info.duration;
        let ffmpeg = ffmpeg().ok_or(PreviewError::FfmpegMissing)?;
        let dir = state
            .folder
            .session_dir()
            .join(format!("thumbs-{}", state.next_thumbnails));
        state.next_thumbnails += 1;
        drop(state);

        // Not cancellable: one pass takes seconds, and a replaced source's folder is deleted
        // below once it ends.
        let result = std::fs::create_dir(&dir)
            .map_err(|err| PreviewError::ThumbnailsFailed {
                reason: err.to_string(),
            })
            .and_then(|()| thumbnails::make(&ffmpeg, &input, duration, &dir));

        let mut state = lock(&self.state);
        let shut_down = state.shut_down;
        let current = state
            .current
            .as_mut()
            .filter(|c| c.source == source && !shut_down);
        match (result, current) {
            (Ok(files), Some(current)) => match &current.thumbnails {
                // Another request finished first; keep one folder.
                Some(kept) => {
                    let _ = std::fs::remove_dir_all(&dir);
                    Ok(kept.files.clone())
                }
                None => {
                    current.thumbnails = Some(Thumbnails {
                        dir,
                        files: files.clone(),
                    });
                    Ok(files)
                }
            },
            (result, current) => {
                let _ = std::fs::remove_dir_all(&dir);
                match (result, current) {
                    (Err(err), Some(_)) => Err(err),
                    _ => Err(PreviewError::Cancelled),
                }
            }
        }
    }

    /// Stops any proxy build, waiting for ffmpeg to exit so it holds no file open, then deletes
    /// the temp folder. Later calls to [`Self::proxy`] fail as cancelled. Idempotent.
    pub fn shut_down(&self) -> std::io::Result<()> {
        let mut state = lock(&self.state);
        state.shut_down = true;
        if let Some(current) = state.current.take() {
            discard(current.proxy, true);
        }
        state.folder.remove()
    }
}

/// Gives up a source that is no longer open: its proxy, as [`discard`] does, and its thumbnails.
fn release(current: Current, wait: bool) {
    discard(current.proxy, wait);
    if let Some(thumbnails) = current.thumbnails {
        let _ = std::fs::remove_dir_all(thumbnails.dir);
    }
}

/// Stops a build or deletes a finished proxy that no source needs any more. With `wait`, a build
/// is stopped before this returns rather than in the background.
fn discard(proxy: Proxy, wait: bool) {
    match proxy {
        Proxy::None => {}
        // The build's own thread deletes its partial file once ffmpeg has exited.
        Proxy::Building(build) => build.job.cancel(wait),
        Proxy::Ready(path) => {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // State stays consistent across a panic in a progress callback, so carry on with it.
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[cfg(test)]
mod tests;

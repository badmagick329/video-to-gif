//! Running an export: the use case that turns a checked [`ExportPlan`] into outputs, one ffmpeg run
//! per ticked file type, and the app-wide state that lets one export run at a time and be
//! cancelled.
//!
//! Progress and outputs go to callbacks rather than Tauri events, so the whole run is tested
//! without an app; `presentation` adapts them to events.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use crate::export::domain::args::{SinglePass, job_args};
use crate::export::domain::memory::MemoryEstimate;
use crate::export::domain::outcome::{ExportError, ExportOutput, ExportProgress};
use crate::export::domain::plan::{ExportPlan, output_name};
use crate::export::domain::progress::ProgressMeter;
use crate::export::domain::settings::FileType;
use crate::export::infrastructure::captions::CaptionFile;
use crate::export::infrastructure::job::{Job, JobEnd};
use crate::export::infrastructure::outputs;

/// How long shutting down waits for a cancelled export to delete its partial file.
const SHUT_DOWN_WAIT: Duration = Duration::from_secs(5);

/// One export's cancel switch and the ffmpeg it is running.
#[derive(Debug, Default)]
pub struct ExportControl {
    state: Mutex<ControlState>,
    ended: Condvar,
}

#[derive(Debug, Default)]
struct ControlState {
    cancelled: bool,
    job: Option<Arc<Job>>,
    ended: bool,
}

impl ExportControl {
    /// Stops the export: kills the running ffmpeg, whose run then reports cancelled and deletes
    /// its partial file, and keeps later file types from starting. With `wait`, returns once
    /// ffmpeg has exited.
    pub fn cancel(&self, wait: bool) {
        let job = {
            let mut state = lock(&self.state);
            state.cancelled = true;
            state.job.clone()
        };
        if let Some(job) = job {
            job.cancel(wait);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        lock(&self.state).cancelled
    }

    /// Records `job` as the running ffmpeg. A cancel that arrived between the last check and now
    /// kills it at once, so no job outlives a cancel.
    fn attach(&self, job: &Arc<Job>) {
        let mut state = lock(&self.state);
        if state.cancelled {
            job.cancel(false);
        }
        state.job = Some(Arc::clone(job));
    }

    fn detach(&self) {
        lock(&self.state).job = None;
    }

    fn end(&self) {
        lock(&self.state).ended = true;
        self.ended.notify_all();
    }

    /// Waits up to `timeout` for the export to end. Returns whether it did.
    fn wait_ended(&self, timeout: Duration) -> bool {
        let state = lock(&self.state);
        let (state, _) = self
            .ended
            .wait_timeout_while(state, timeout, |state| !state.ended)
            .unwrap_or_else(|poison| poison.into_inner());
        state.ended
    }
}

/// The app's exports: at most one running. Managed by Tauri for the app's lifetime.
#[derive(Debug, Default)]
pub struct Exports {
    current: Mutex<Option<Arc<ExportControl>>>,
}

/// The running export's claim on [`Exports`]; dropping it frees the slot and wakes a waiting
/// shutdown.
#[derive(Debug)]
pub struct Running<'a> {
    exports: &'a Exports,
    control: Arc<ExportControl>,
}

impl Running<'_> {
    pub fn control(&self) -> &ExportControl {
        &self.control
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        let mut current = lock(&self.exports.current);
        if current
            .as_ref()
            .is_some_and(|c| Arc::ptr_eq(c, &self.control))
        {
            *current = None;
        }
        self.control.end();
    }
}

impl Exports {
    /// Claims the slot for a new export, or fails with [`ExportError::Busy`] while one runs.
    pub fn begin(&self) -> Result<Running<'_>, ExportError> {
        let mut current = lock(&self.current);
        if current.is_some() {
            return Err(ExportError::Busy);
        }
        let control = Arc::new(ExportControl::default());
        *current = Some(Arc::clone(&control));
        Ok(Running {
            exports: self,
            control,
        })
    }

    pub fn is_running(&self) -> bool {
        lock(&self.current).is_some()
    }

    /// Cancels the running export, if any. Returns at once; the export reports its outputs as
    /// cancelled when ffmpeg has exited.
    pub fn cancel(&self) {
        if let Some(control) = lock(&self.current).clone() {
            control.cancel(false);
        }
    }

    /// Cancels the running export and waits, up to a few seconds, for it to delete its partial
    /// file and caption image, so nothing is left behind when the app exits.
    pub fn shut_down(&self) {
        let control = lock(&self.current).clone();
        if let Some(control) = control {
            control.cancel(true);
            control.wait_ended(SHUT_DOWN_WAIT);
        }
    }
}

/// Where a run finds its tools and puts its files.
#[derive(Debug, Clone, Copy)]
pub struct Environment<'a> {
    pub ffmpeg: &'a Path,
    /// This session's temp folder, for the caption image.
    pub work_dir: &'a Path,
    /// Free physical memory in bytes, `None` when unknown.
    pub available_memory: Option<u64>,
}

/// Runs `plan`: refuses it when its frames would not fit in free memory, writes the caption image,
/// then makes each ticked file type in order (GIF, WebP, MP4), one ffmpeg at a time. A file type
/// that fails is reported and the rest still run, as in mgck. Returns one [`ExportOutput`] per
/// file type, each also passed to `on_output` as it settles.
///
/// Each output is written under a temporary name in the destination folder and renamed when
/// ffmpeg succeeds; on failure or cancel the partial file is deleted, so the folder only ever
/// gains finished outputs. The caption image is deleted when the run ends, however it ends.
pub fn run(
    plan: &ExportPlan,
    env: Environment<'_>,
    control: &ExportControl,
    mut on_progress: impl FnMut(ExportProgress),
    mut on_output: impl FnMut(&ExportOutput),
) -> Result<Vec<ExportOutput>, ExportError> {
    let estimate = MemoryEstimate::new(&plan.selection, env.available_memory);
    if !estimate.fits {
        return Err(ExportError::NotEnoughMemory {
            held_bytes: estimate.held_bytes,
            available_bytes: env.available_memory.unwrap_or_default(),
        });
    }
    let captions = plan
        .captions
        .as_deref()
        .map(|png| CaptionFile::write(env.work_dir, png))
        .transpose()
        .map_err(|err| ExportError::CaptionsFailed {
            reason: err.to_string(),
        })?;
    let source_name = plan
        .source
        .file_name()
        .map_or_else(|| "output".into(), |name| name.to_string_lossy());

    let mut results = Vec::new();
    for &file_type in &plan.selection.settings.file_types {
        let result = if control.is_cancelled() {
            ExportOutput::Cancelled { file_type }
        } else {
            make_output(
                plan,
                env.ffmpeg,
                file_type,
                &output_name(&source_name, file_type),
                captions.as_ref().map(CaptionFile::path),
                control,
                &mut on_progress,
            )
        };
        on_output(&result);
        results.push(result);
    }
    Ok(results)
}

/// One file type: run ffmpeg into a temporary file, then publish or delete it.
fn make_output(
    plan: &ExportPlan,
    ffmpeg: &Path,
    file_type: FileType,
    name: &str,
    captions: Option<&Path>,
    control: &ExportControl,
    on_progress: &mut impl FnMut(ExportProgress),
) -> ExportOutput {
    let temp = outputs::temp_path(&plan.destination, name);
    let args = job_args(&SinglePass::from_plan(plan, file_type, &temp, captions));
    let (job, output) = match Job::start(ffmpeg, &args) {
        Ok(started) => started,
        Err(reason) => return ExportOutput::Failed { file_type, reason },
    };
    let job = Arc::new(job);
    control.attach(&job);
    let mut report = |progress: f64| {
        on_progress(ExportProgress {
            file_type,
            progress,
        })
    };
    report(0.0);
    let meter = ProgressMeter::new(&plan.selection, file_type);
    let end = job.finish(output, meter, &mut report);
    control.detach();

    match end {
        JobEnd::Succeeded => match publish(&temp, &plan.destination, name) {
            Ok((path, size)) => {
                report(1.0);
                ExportOutput::Done {
                    file_type,
                    path: path.display().to_string(),
                    size,
                    width: plan.selection.size.width,
                    height: plan.selection.size.height,
                }
            }
            Err(err) => {
                outputs::discard(&temp);
                ExportOutput::Failed {
                    file_type,
                    reason: format!("could not save {name}: {err}"),
                }
            }
        },
        JobEnd::Failed { reason } => {
            outputs::discard(&temp);
            ExportOutput::Failed { file_type, reason }
        }
        JobEnd::Cancelled => {
            outputs::discard(&temp);
            ExportOutput::Cancelled { file_type }
        }
    }
}

fn publish(temp: &Path, dir: &Path, name: &str) -> std::io::Result<(PathBuf, u64)> {
    let size = std::fs::metadata(temp)?.len();
    let path = outputs::publish(temp, dir, name)?;
    Ok((path, size))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // The guarded state stays consistent across a panic in a callback, so carry on with it.
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

pub mod estimate;
pub mod session_outputs;

#[cfg(test)]
mod tests;

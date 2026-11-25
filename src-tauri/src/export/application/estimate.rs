//! Estimating sizes: the use case that runs a checked [`ExportPlan`]'s sample encodes, every
//! ticked file type at once, and the app-wide state that lets a newer estimate, playback or an
//! export cancel the running one.
//!
//! Samples are the user's own export at below-normal priority, written to the session's temp
//! folder and deleted as soon as they are measured. The methods are in
//! [`crate::export::domain::estimate`]. For a selection up to 20 s, GIF and WebP share one ffmpeg
//! that decodes it once; a longer one gets a run per file type (a run per window for WebP). MP4
//! and a gifski GIF (an ffmpeg piping to gifski) always run on their own. Everything runs at once
//! and each file type settles as it finishes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Environment, lock};
use crate::export::domain::args::{
    InputTrim, SharedOutput, SinglePass, Span, gifski_args, sample_args, shared_sample_args,
    windowed_sample_args,
};
use crate::export::domain::estimate::{
    Method, SizeEstimate, WebpSample, gif_estimate, output_frames, sample_windows, webp_estimate,
    webp_layout,
};
use crate::export::domain::memory::MemoryEstimate;
use crate::export::domain::plan::ExportPlan;
use crate::export::domain::progress::ENCODED;
use crate::export::domain::settings::FileType;
use crate::export::infrastructure::captions::CaptionFile;
use crate::export::infrastructure::job::{Job, JobEnd};
use crate::process::Priority;

/// Numbers sample files, so a cancelled estimate still deleting its files never shares a name
/// with the next one's.
static NEXT_SAMPLE: AtomicU64 = AtomicU64::new(1);

/// One estimate's cancel switch and the ffmpeg runs it has going.
#[derive(Debug, Default)]
pub struct EstimateControl {
    state: Mutex<ControlState>,
}

#[derive(Debug, Default)]
struct ControlState {
    cancelled: bool,
    jobs: Vec<Arc<Job>>,
}

impl EstimateControl {
    /// Kills every running sample and keeps new ones from starting. With `wait`, returns once each
    /// ffmpeg has exited.
    pub fn cancel(&self, wait: bool) {
        let jobs = {
            let mut state = lock(&self.state);
            state.cancelled = true;
            state.jobs.clone()
        };
        for job in jobs {
            job.cancel(wait);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        lock(&self.state).cancelled
    }

    /// Records a started sample. One started after a cancel is killed at once.
    fn attach(&self, job: &Arc<Job>) {
        let mut state = lock(&self.state);
        if state.cancelled {
            job.cancel(false);
        }
        state.jobs.push(Arc::clone(job));
    }

    fn detach(&self, job: &Arc<Job>) {
        lock(&self.state).jobs.retain(|j| !Arc::ptr_eq(j, job));
    }
}

/// The app's size estimates: at most one running, since any change makes the running one stale.
/// Managed by Tauri for the app's lifetime.
#[derive(Debug, Default)]
pub struct Estimates {
    current: Mutex<Option<Arc<EstimateControl>>>,
}

impl Estimates {
    /// Cancels the running estimate, if any, and makes a new one current.
    pub fn begin(&self) -> Arc<EstimateControl> {
        let control = Arc::new(EstimateControl::default());
        let previous = lock(&self.current).replace(Arc::clone(&control));
        if let Some(previous) = previous {
            previous.cancel(false);
        }
        control
    }

    /// Cancels the running estimate, if any. With `wait`, returns once its ffmpeg runs have
    /// exited, so the temp folder can be deleted.
    pub fn cancel(&self, wait: bool) {
        if let Some(control) = lock(&self.current).take() {
            control.cancel(wait);
        }
    }
}

/// Estimates each ticked file type of `plan` at once, passing each [`SizeEstimate`] to
/// `on_estimate` as it settles. Returns them all in run order (GIF, WebP, MP4). `env.work_dir`
/// receives the caption image and the samples, all deleted before this returns.
pub fn estimate(
    plan: &ExportPlan,
    env: Environment<'_>,
    control: &EstimateControl,
    on_estimate: &(impl Fn(&SizeEstimate) + Sync),
) -> Vec<SizeEstimate> {
    let captions = match plan
        .captions
        .as_deref()
        .map(|png| CaptionFile::write(env.work_dir, png))
        .transpose()
    {
        Ok(captions) => captions,
        Err(err) => {
            let reason = format!("could not write the caption image: {err}");
            return plan
                .selection
                .settings
                .file_types
                .iter()
                .map(|&file_type| {
                    let failed = SizeEstimate::Failed {
                        file_type,
                        reason: reason.clone(),
                    };
                    on_estimate(&failed);
                    failed
                })
                .collect();
        }
    };
    let run = Run {
        plan,
        env,
        control,
        captions: captions.as_ref().map(CaptionFile::path),
    };
    let settings = &plan.selection.settings;
    // Short selections share one decode between GIF and WebP. MP4 always runs on its own: a
    // streaming x264 output beside a GIF that holds every frame for its palette deadlocked
    // ffmpeg's scheduler (ffmpeg 9.0, a 1080p H.264 talk; GIF with WebP did not). A gifski GIF
    // does too, since its frames leave through ffmpeg's stdout for a gifski of its own.
    let (shared, own): (Vec<FileType>, Vec<FileType>) =
        if plan.selection.selection_seconds <= SHARED_MAX_SECONDS {
            settings.file_types.iter().partition(|&&file_type| {
                file_type != FileType::Mp4 && !settings.piped_to_gifski(file_type)
            })
        } else {
            (Vec::new(), settings.file_types.clone())
        };
    let run = &run;
    let mut estimates: Vec<SizeEstimate> = thread::scope(|scope| {
        let mut handles = Vec::new();
        if !shared.is_empty() {
            let shared = &shared;
            handles.push(scope.spawn(move || {
                let estimates = run.shared(shared);
                estimates.iter().for_each(on_estimate);
                estimates
            }));
        }
        for &file_type in &own {
            handles.push(scope.spawn(move || {
                let estimate = run.separate(file_type);
                on_estimate(&estimate);
                vec![estimate]
            }));
        }
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("estimate thread panicked"))
            .collect()
    });
    estimates.sort_by_key(SizeEstimate::file_type);
    estimates
}

/// The longest selection whose GIF and WebP share one run ([`shared_sample_args`]). Up to here,
/// decoding the selection once costs no more than decoding each window from its keyframe. Past
/// it, each file type runs on its own and windows decode only around themselves.
const SHARED_MAX_SECONDS: f64 = 20.0;

/// What every sample of one estimate shares.
struct Run<'a> {
    plan: &'a ExportPlan,
    env: Environment<'a>,
    control: &'a EstimateControl,
    captions: Option<&'a Path>,
}

/// A sample's file and the frames that entered its encoder.
struct Sample {
    bytes: Vec<u8>,
    frames: u64,
}

/// Why a sample gave no measurement.
#[derive(Debug, Clone)]
enum Stopped {
    Cancelled,
    Failed(String),
}

/// What one file type samples, and how its sample becomes bytes.
enum Plan {
    /// The whole selection: the file's size.
    Whole,
    /// GIF windows in one output, sharing a palette: scaled by frames.
    GifWindows(Vec<InputTrim>),
    /// WebP windows, an output each: the first-frame model.
    WebpWindows(Vec<InputTrim>),
}

impl Run<'_> {
    /// What `file_type` samples, or why it cannot be estimated.
    fn plan(&self, file_type: FileType) -> Result<Plan, Stopped> {
        let selection = &self.plan.selection;
        match Method::for_selection(selection, file_type) {
            Method::Whole => {
                // The export itself, into the temp folder: exact. Refused when its frames would
                // not fit in free memory, where the export would page the machine; a background
                // estimate must not.
                let mut alone = selection.clone();
                alone.settings.file_types = vec![file_type];
                if MemoryEstimate::new(&alone, self.env.available_memory).fits {
                    Ok(Plan::Whole)
                } else {
                    Err(Stopped::Failed(
                        "encoding the whole selection would hold more frames than fit in free \
                         memory"
                            .to_owned(),
                    ))
                }
            }
            Method::Sampled(windows) => {
                let windows = sample_windows(selection, windows);
                Ok(match file_type {
                    FileType::Gif => Plan::GifWindows(windows),
                    FileType::Webp => Plan::WebpWindows(windows),
                    FileType::Mp4 => Plan::Whole,
                })
            }
        }
    }

    /// Turns a file type's samples into its estimate.
    fn conclude(&self, file_type: FileType, plan: &Plan, samples: &[Sample]) -> SizeEstimate {
        let output_frames = output_frames(&self.plan.selection);
        let bytes = match plan {
            Plan::Whole => Some(samples[0].bytes.len() as u64),
            Plan::GifWindows(_) => Some(gif_estimate(
                samples[0].bytes.len() as u64,
                samples[0].frames,
                output_frames,
            )),
            Plan::WebpWindows(_) => samples
                .iter()
                .map(|sample| {
                    webp_layout(&sample.bytes).map(|layout| WebpSample {
                        layout,
                        frames: sample.frames,
                    })
                })
                .collect::<Option<Vec<_>>>()
                .and_then(|samples| webp_estimate(&samples, output_frames)),
        };
        match bytes {
            Some(bytes) => SizeEstimate::Done { file_type, bytes },
            None => SizeEstimate::Failed {
                file_type,
                reason: "a WebP sample was not a WebP file".to_owned(),
            },
        }
    }

    /// `file_types` (GIF and WebP) from one ffmpeg that decodes the selection once.
    fn shared(&self, file_types: &[FileType]) -> Vec<SizeEstimate> {
        let input_trim = SinglePass::from_plan(self.plan, FileType::Gif, Path::new(""), None).trim;
        let input_start = input_trim.map_or(0.0, |trim| trim.seek);
        let span = |window: &InputTrim| Span {
            start: ((window.seek - input_start) * 1e6).round() / 1e6,
            duration: window.duration,
        };

        // Each planned file type with its outputs' indexes in the run.
        let mut planned: Vec<(FileType, Plan, Vec<usize>)> = Vec::new();
        let mut settled: Vec<Option<SizeEstimate>> = vec![None; file_types.len()];
        let mut files: Vec<SampleFile> = Vec::new();
        let mut spans: Vec<Option<Vec<Span>>> = Vec::new();
        for (slot, &file_type) in file_types.iter().enumerate() {
            let plan = match self.plan(file_type) {
                Ok(plan) => plan,
                Err(stopped) => {
                    settled[slot] = Some(stopped.into_estimate(file_type));
                    continue;
                }
            };
            let outputs: Vec<Option<Vec<Span>>> = match &plan {
                Plan::Whole => vec![None],
                Plan::GifWindows(windows) => vec![Some(windows.iter().map(span).collect())],
                Plan::WebpWindows(windows) => windows.iter().map(|w| Some(vec![span(w)])).collect(),
            };
            let indexes = (spans.len()..spans.len() + outputs.len()).collect();
            for output in outputs {
                files.push(SampleFile::new(self.env.work_dir, file_type));
                spans.push(output);
            }
            planned.push((file_type, plan, indexes));
        }

        if !planned.is_empty() {
            let file_type_of = |index: usize| {
                planned
                    .iter()
                    .find(|(_, _, indexes)| indexes.contains(&index))
                    .map(|(file_type, ..)| *file_type)
                    .expect("every output belongs to a file type")
            };
            let outputs: Vec<SharedOutput<'_>> = spans
                .iter()
                .enumerate()
                .map(|(index, spans)| SharedOutput {
                    pass: SinglePass::from_plan(
                        self.plan,
                        file_type_of(index),
                        &files[index].0,
                        self.captions,
                    ),
                    spans: spans.clone(),
                    key: format!("s{index}"),
                })
                .collect();
            let args = shared_sample_args(&self.plan.source, input_trim, self.captions, &outputs);
            let mut frames = vec![0u64; outputs.len()];
            let ended = self.run_job(&args, None, "shared run", |line| {
                let index = line
                    .split_once('=')
                    .and_then(|(key, _)| key.strip_prefix('s'))
                    .and_then(|index| index.parse::<usize>().ok());
                if let Some(count) = index.and_then(|index| frames.get_mut(index)) {
                    *count += 1;
                }
            });
            for (file_type, plan, indexes) in &planned {
                let slot = file_types
                    .iter()
                    .position(|t| t == file_type)
                    .expect("planned from the ticked file types");
                let samples: Result<Vec<Sample>, Stopped> = ended.clone().and_then(|()| {
                    indexes
                        .iter()
                        .map(|&index| files[index].read(frames[index]))
                        .collect()
                });
                settled[slot] = Some(match samples {
                    Ok(samples) => self.conclude(*file_type, plan, &samples),
                    Err(stopped) => stopped.into_estimate(*file_type),
                });
            }
        }
        settled
            .into_iter()
            .map(|estimate| estimate.expect("every file type settled"))
            .collect()
    }

    /// One file type from its own ffmpeg runs: GIF's windows in one (piped to gifski for a gifski
    /// GIF), WebP's windows a run each, all at once.
    fn separate(&self, file_type: FileType) -> SizeEstimate {
        let plan = match self.plan(file_type) {
            Ok(plan) => plan,
            Err(stopped) => return stopped.into_estimate(file_type),
        };
        let samples: Result<Vec<Sample>, Stopped> = match &plan {
            Plan::Whole => self.sample(file_type, None, sample_args).map(|s| vec![s]),
            Plan::GifWindows(windows) => self
                .sample(file_type, None, |pass| windowed_sample_args(pass, windows))
                .map(|s| vec![s]),
            Plan::WebpWindows(windows) => thread::scope(|scope| {
                let handles: Vec<_> = windows
                    .iter()
                    .map(|&window| {
                        scope.spawn(move || self.sample(file_type, Some(window), sample_args))
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().expect("WebP sample thread panicked"))
                    .collect()
            }),
        };
        match samples {
            Ok(samples) => self.conclude(file_type, &plan, &samples),
            Err(stopped) => stopped.into_estimate(file_type),
        }
    }

    /// Runs one sample of `file_type`, over `window` instead of the selection when given, with the
    /// arguments `args` builds from the export's pass. Reads the file, then deletes it.
    fn sample(
        &self,
        file_type: FileType,
        window: Option<InputTrim>,
        args: impl FnOnce(&SinglePass<'_>) -> Vec<OsString>,
    ) -> Result<Sample, Stopped> {
        let file = SampleFile::new(self.env.work_dir, file_type);
        let mut pass = SinglePass::from_plan(self.plan, file_type, &file.0, self.captions);
        if window.is_some() {
            pass.trim = window;
        }
        let mut what = match window {
            Some(window) => format!("{} at {} s", file_type.extension(), window.seek),
            None => file_type.extension().to_owned(),
        };
        let settings = &self.plan.selection.settings;
        let gifski = if settings.piped_to_gifski(file_type) {
            let gifski = self
                .env
                .gifski
                .ok_or_else(|| Stopped::Failed("gifski was not found on PATH".to_owned()))?;
            what.push_str(" and gifski");
            let width = self.plan.selection.size.width;
            Some((gifski, gifski_args(settings, width, &file.0)))
        } else {
            None
        };
        let mut frames = 0;
        let gifski = gifski.as_ref().map(|(path, args)| (*path, args.as_slice()));
        self.run_job(&args(&pass), gifski, &what, |line| {
            if line.starts_with(ENCODED) {
                frames += 1;
            }
        })?;
        file.read(frames)
    }

    /// Runs ffmpeg with `args` in the background until it exits, piping its frames to `gifski`
    /// with its arguments when given, and handing each progress line to `on_line`, unless the
    /// estimate is cancelled first. `what` names the run in the spawn log. A cancel kills both
    /// processes, so a stalled pipe cannot outlive the estimate.
    fn run_job(
        &self,
        args: &[OsString],
        gifski: Option<(&Path, &[OsString])>,
        what: &str,
        on_line: impl FnMut(&str),
    ) -> Result<(), Stopped> {
        if self.control.is_cancelled() {
            return Err(Stopped::Cancelled);
        }
        let started = match gifski {
            Some((gifski, gifski_args)) => Job::start_piped(
                self.env.ffmpeg,
                args,
                gifski,
                gifski_args,
                Priority::Background,
            ),
            None => Job::start_in_background(self.env.ffmpeg, args),
        };
        let (job, output) = started.map_err(Stopped::Failed)?;
        log_spawn(what);
        let job = Arc::new(job);
        self.control.attach(&job);
        let end = job.finish_lines(output, on_line);
        self.control.detach(&job);
        match end {
            JobEnd::Succeeded => Ok(()),
            JobEnd::Failed { reason } => Err(Stopped::Failed(reason)),
            JobEnd::Cancelled => Err(Stopped::Cancelled),
        }
    }
}

impl Stopped {
    fn into_estimate(self, file_type: FileType) -> SizeEstimate {
        match self {
            Stopped::Cancelled => SizeEstimate::Cancelled { file_type },
            Stopped::Failed(reason) => SizeEstimate::Failed { file_type, reason },
        }
    }
}

/// A sample's path in the temp folder; dropping it deletes the file. ffmpeg has exited by then,
/// so nothing holds it open, and one that cannot be deleted goes with the session folder at exit.
struct SampleFile(PathBuf);

impl SampleFile {
    fn new(dir: &Path, file_type: FileType) -> Self {
        Self(dir.join(format!(
            "estimate-{}.{}",
            NEXT_SAMPLE.fetch_add(1, Ordering::Relaxed),
            file_type.extension()
        )))
    }

    fn read(&self, frames: u64) -> Result<Sample, Stopped> {
        let bytes = std::fs::read(&self.0)
            .map_err(|err| Stopped::Failed(format!("could not read a sample: {err}")))?;
        Ok(Sample { bytes, frames })
    }
}

impl Drop for SampleFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Logs each estimate ffmpeg to stderr with a millisecond clock, so the scheduling (nothing starts
/// until 1.5 s after the last change) can be checked from the debug build's console.
fn log_spawn(what: &str) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis());
    eprintln!("[{millis}] estimate: started ffmpeg for {what}");
}

#[cfg(test)]
mod tests;

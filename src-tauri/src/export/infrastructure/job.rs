//! Running one job: an ffmpeg, or an ffmpeg piping its frames to gifski. Streaming its progress,
//! stopping it on demand, and keeping the tail of the failing process's error output for the
//! report.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use crate::export::domain::progress::ProgressMeter;
use crate::process::{self, Priority};

/// Lines of stderr kept for a failure's reason. With `-loglevel error` these are ffmpeg's own
/// error lines, the last of which usually names the cause and the earlier ones the filter or
/// stream involved.
const STDERR_TAIL_LINES: usize = 6;

/// How often the waiting thread looks for exited processes. Short enough that an exit is noticed
/// at once by a person, long enough to cost nothing.
const POLL: Duration = Duration::from_millis(20);

/// A running job: ffmpeg alone, or ffmpeg feeding gifski. Shared between the thread that waits on
/// it and whoever may cancel it.
#[derive(Debug)]
pub struct Job {
    /// ffmpeg first, then gifski when there is one.
    stages: Mutex<Vec<Stage>>,
    cancelled: AtomicBool,
}

#[derive(Debug)]
struct Stage {
    program: &'static str,
    child: Child,
    exit: Option<ExitStatus>,
}

/// What [`Job::start`] hands to the waiting thread alongside the job.
pub struct JobOutput {
    /// The stream of `<key>=<value>` progress lines: ffmpeg's stdout, or its stderr when stdout
    /// carries the frames to gifski.
    reports: Box<dyn Read + Send>,
    /// Whether `reports` is ffmpeg's stderr, so its other lines are ffmpeg's errors.
    reports_hold_errors: bool,
    /// Each stage's drained stderr, in stage order; `None` for the one read as `reports`.
    stderr: Vec<Option<thread::JoinHandle<String>>>,
}

/// How a job ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEnd {
    Succeeded,
    /// A process could not start or exited non-zero; `reason` is the tail of the error output of
    /// the one that failed first (gifski's prefixed with its name).
    Failed {
        reason: String,
    },
    Cancelled,
}

impl Job {
    /// Starts `ffmpeg` with `args`, which write their progress lines to stdout (`job_args`).
    pub fn start(ffmpeg: &Path, args: &[OsString]) -> Result<(Self, JobOutput), String> {
        Self::start_at(ffmpeg, args, Priority::Normal)
    }

    /// [`Self::start`] at below-normal priority, for work the user did not ask for (size
    /// estimates), so it never slows the player, an export or the rest of the PC.
    pub fn start_in_background(
        ffmpeg: &Path,
        args: &[OsString],
    ) -> Result<(Self, JobOutput), String> {
        Self::start_at(ffmpeg, args, Priority::Background)
    }

    fn start_at(
        ffmpeg: &Path,
        args: &[OsString],
        priority: Priority,
    ) -> Result<(Self, JobOutput), String> {
        let mut child = process::command_at(ffmpeg, priority)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("ffmpeg could not start: {err}"))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = drain(child.stderr.take().expect("stderr is piped"));
        let job = Self::new(vec![Stage::new("ffmpeg", child)]);
        let output = JobOutput {
            reports: Box::new(stdout),
            reports_hold_errors: false,
            stderr: vec![Some(stderr)],
        };
        Ok((job, output))
    }

    /// Starts `ffmpeg` with `ffmpeg_args`, which write Y4M frames to stdout and their progress
    /// lines to stderr, and `gifski` with `gifski_args`, reading those frames from stdin. The OS
    /// pipe between them is what paces ffmpeg: it can only run as far ahead of gifski as the pipe
    /// holds, so ffmpeg's frame times measure gifski's progress too.
    pub fn start_piped(
        ffmpeg: &Path,
        ffmpeg_args: &[OsString],
        gifski: &Path,
        gifski_args: &[OsString],
        priority: Priority,
    ) -> Result<(Self, JobOutput), String> {
        let mut first = process::command_at(ffmpeg, priority)
            .args(ffmpeg_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("ffmpeg could not start: {err}"))?;
        let frames = first.stdout.take().expect("stdout is piped");
        let second = process::command_at(gifski, priority)
            .args(gifski_args)
            .stdin(Stdio::from(frames))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn();
        let mut second = match second {
            Ok(child) => child,
            Err(err) => {
                let _ = first.kill();
                let _ = first.wait();
                return Err(format!("gifski could not start: {err}"));
            }
        };
        let reports = first.stderr.take().expect("stderr is piped");
        let gifski_stderr = drain(second.stderr.take().expect("stderr is piped"));
        let job = Self::new(vec![
            Stage::new("ffmpeg", first),
            Stage::new("gifski", second),
        ]);
        let output = JobOutput {
            reports: Box::new(reports),
            reports_hold_errors: true,
            stderr: vec![None, Some(gifski_stderr)],
        };
        Ok((job, output))
    }

    fn new(stages: Vec<Stage>) -> Self {
        Self {
            stages: Mutex::new(stages),
            cancelled: AtomicBool::new(false),
        }
    }

    /// Reads the job's progress lines through `meter` until ffmpeg exits, calling `on_progress`
    /// with each rise in progress (0..=1), and reports how the job ended. Returns only once every
    /// process has exited, so the output file is closed and can be renamed or deleted.
    pub fn finish(
        &self,
        output: JobOutput,
        mut meter: ProgressMeter,
        mut on_progress: impl FnMut(f64),
    ) -> JobEnd {
        self.finish_lines(output, |line| {
            if let Some(progress) = meter.read_line(line) {
                on_progress(progress);
            }
        })
    }

    /// [`Self::finish`], handing each progress line to `on_line` as it arrives.
    pub fn finish_lines(&self, output: JobOutput, mut on_line: impl FnMut(&str)) -> JobEnd {
        let mut shared_errors = String::new();
        let first_failure = thread::scope(|scope| {
            // Watches every process while the lines are read, so the first to fail is known (a
            // gifski that dies makes ffmpeg fail on the broken pipe a moment later) and the rest
            // are stopped rather than left to finish an output that is lost anyway.
            let waiter = scope.spawn(|| self.wait_all());
            for line in BufReader::new(output.reports).lines() {
                let Ok(line) = line else { break };
                if is_report(&line) {
                    on_line(&line);
                } else if output.reports_hold_errors {
                    shared_errors.push_str(&line);
                    shared_errors.push('\n');
                }
            }
            waiter.join().expect("job waiter panicked")
        });
        let mut stderr: Vec<String> = output
            .stderr
            .into_iter()
            .map(|handle| handle.map_or_else(String::new, |h| h.join().unwrap_or_default()))
            .collect();
        if self.cancelled.load(Ordering::SeqCst) {
            return JobEnd::Cancelled;
        }
        match first_failure {
            Ok(None) => JobEnd::Succeeded,
            Ok(Some((index, program, status))) => {
                let text = if output.reports_hold_errors && index == 0 {
                    shared_errors
                } else {
                    std::mem::take(&mut stderr[index])
                };
                let reason = tail(&text, STDERR_TAIL_LINES)
                    .unwrap_or_else(|| format!("{program} exited with {status}"));
                JobEnd::Failed {
                    reason: if program == "ffmpeg" {
                        reason
                    } else {
                        format!("{program}: {reason}")
                    },
                }
            }
            Err(err) => JobEnd::Failed {
                reason: err.to_string(),
            },
        }
    }

    /// Stops every process; the thread in [`Self::finish`] then reports [`JobEnd::Cancelled`].
    /// With `wait`, returns only once they have all exited.
    pub fn cancel(&self, wait: bool) {
        self.cancelled.store(true, Ordering::SeqCst);
        let mut stages = self.lock_stages();
        for stage in stages.iter_mut() {
            // Fails only if the process already exited, which is what cancelling wants anyway.
            let _ = stage.child.kill();
        }
        if wait {
            for stage in stages.iter_mut() {
                let _ = stage.child.wait();
            }
        }
    }

    /// Waits for every process to exit by polling, so the lock is free between polls and
    /// [`Self::cancel`] can kill them at any moment. Returns the first process seen to exit with
    /// a failure, by index and name, with its status; once one has, the others are killed.
    #[allow(clippy::type_complexity)]
    fn wait_all(&self) -> std::io::Result<Option<(usize, &'static str, ExitStatus)>> {
        let mut first_failure = None;
        loop {
            {
                let mut stages = self.lock_stages();
                for (index, stage) in stages.iter_mut().enumerate() {
                    if stage.exit.is_none()
                        && let Some(status) = stage.child.try_wait()?
                    {
                        stage.exit = Some(status);
                        if !status.success() && first_failure.is_none() {
                            first_failure = Some((index, stage.program, status));
                        }
                    }
                }
                if first_failure.is_some() {
                    for stage in stages.iter_mut().filter(|stage| stage.exit.is_none()) {
                        let _ = stage.child.kill();
                    }
                }
                if stages.iter().all(|stage| stage.exit.is_some()) {
                    return Ok(first_failure);
                }
            }
            thread::sleep(POLL);
        }
    }

    /// The process ids of ffmpeg and gifski, for tests that check none outlives the job.
    #[cfg(test)]
    pub fn pids(&self) -> Vec<u32> {
        self.lock_stages().iter().map(|s| s.child.id()).collect()
    }

    fn lock_stages(&self) -> MutexGuard<'_, Vec<Stage>> {
        // A panic elsewhere while holding the lock leaves the children themselves intact.
        self.stages
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl Stage {
    fn new(program: &'static str, child: Child) -> Self {
        Self {
            program,
            child,
            exit: None,
        }
    }
}

/// Reads `stream` to its end on its own thread, so a chatty process never blocks on a full pipe
/// while the caller reads another.
fn drain(mut stream: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Whether `line` is a `<key>=<value>` report from `-stats_enc_pre` rather than an error message:
/// keys are single words, and ffmpeg's error lines are sentences or start with a `[context]`.
fn is_report(line: &str) -> bool {
    line.split_once('=').is_some_and(|(key, _)| {
        !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// The last `lines` non-empty lines of `text`, joined by newlines.
fn tail(text: &str, lines: usize) -> Option<String> {
    let kept: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    let start = kept.len().saturating_sub(lines);
    (!kept.is_empty()).then(|| kept[start..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{is_report, tail};

    #[test]
    fn a_failures_reason_is_the_last_lines_of_stderr() {
        let stderr = "\nfirst\n\nsecond\nthird\n";
        assert_eq!(tail(stderr, 2).as_deref(), Some("second\nthird"));
        assert_eq!(tail(stderr, 10).as_deref(), Some("first\nsecond\nthird"));
        assert_eq!(tail(" \n", 3), None);
    }

    #[test]
    fn progress_reports_are_told_apart_from_ffmpegs_error_lines() {
        assert!(is_report("encoded=1.5"));
        assert!(is_report("s12=N/A"));
        assert!(is_report("tapped=0"));
        assert!(!is_report(
            "[vf#0:0 @ 000001] Error: max_colors=1 is out of range"
        ));
        assert!(!is_report("Error opening input file x=1.mp4."));
        assert!(!is_report("no equals sign here"));
    }
}

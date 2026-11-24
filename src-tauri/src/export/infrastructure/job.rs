//! Running one ffmpeg job: streaming its progress, stopping it on demand, and keeping the tail of
//! its error output for the report.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStdout, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use crate::preview::domain::progress_from_line;
use crate::process;

/// Lines of stderr kept for a failure's reason. With `-loglevel error` these are ffmpeg's own
/// error lines, the last of which usually names the cause and the earlier ones the filter or
/// stream involved.
const STDERR_TAIL_LINES: usize = 6;

/// A running ffmpeg. Shared between the thread that waits on it and whoever may cancel it.
#[derive(Debug)]
pub struct Job {
    child: Mutex<Child>,
    cancelled: AtomicBool,
}

/// What [`Job::start`] hands to the waiting thread alongside the job.
#[derive(Debug)]
pub struct JobOutput {
    stdout: ChildStdout,
    stderr: thread::JoinHandle<String>,
}

/// How a job ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobEnd {
    Succeeded,
    /// ffmpeg could not start or exited non-zero; `reason` is the tail of its error output.
    Failed {
        reason: String,
    },
    Cancelled,
}

impl Job {
    /// Starts `ffmpeg` with `args`, which must write progress to stdout (`-progress pipe:1`).
    pub fn start(ffmpeg: &Path, args: &[OsString]) -> Result<(Self, JobOutput), String> {
        let mut child = process::command(ffmpeg)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| format!("ffmpeg could not start: {err}"))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        // Drained on its own thread so a chatty ffmpeg never blocks on a full stderr pipe while
        // the caller reads stdout.
        let stderr = thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).into_owned()
        });
        let job = Self {
            child: Mutex::new(child),
            cancelled: AtomicBool::new(false),
        };
        Ok((job, JobOutput { stdout, stderr }))
    }

    /// Reads progress until ffmpeg exits, calling `on_progress` with 0..=1 of `expected_seconds`
    /// of output, and reports how the job ended. Returns only once ffmpeg has exited, so its
    /// output file is closed and can be renamed or deleted.
    pub fn finish(
        &self,
        output: JobOutput,
        expected_seconds: f64,
        mut on_progress: impl FnMut(f64),
    ) -> JobEnd {
        for line in BufReader::new(output.stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(progress) = progress_from_line(&line, expected_seconds) {
                on_progress(progress);
            }
        }
        let status = self.wait();
        let stderr = output.stderr.join().unwrap_or_default();
        if self.cancelled.load(Ordering::SeqCst) {
            return JobEnd::Cancelled;
        }
        match status {
            Ok(status) if status.success() => JobEnd::Succeeded,
            Ok(status) => JobEnd::Failed {
                reason: tail(&stderr, STDERR_TAIL_LINES)
                    .unwrap_or_else(|| format!("ffmpeg exited with {status}")),
            },
            Err(err) => JobEnd::Failed {
                reason: err.to_string(),
            },
        }
    }

    /// Stops ffmpeg; the thread in [`Self::finish`] then reports [`JobEnd::Cancelled`]. With
    /// `wait`, returns only once ffmpeg has exited.
    pub fn cancel(&self, wait: bool) {
        self.cancelled.store(true, Ordering::SeqCst);
        let mut child = self.lock_child();
        // Fails only if ffmpeg already exited, which is what cancelling wants anyway.
        let _ = child.kill();
        if wait {
            let _ = child.wait();
        }
    }

    /// Waits for ffmpeg to exit by polling, so the lock is free between polls and
    /// [`Self::cancel`] can kill it at any moment. stdout has already closed, so ffmpeg is at most
    /// finishing its output file (MP4's faststart pass rewrites it).
    fn wait(&self) -> std::io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.lock_child().try_wait()? {
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn lock_child(&self) -> MutexGuard<'_, Child> {
        // A panic elsewhere while holding the lock leaves the Child itself intact.
        self.child
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
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
    use super::tail;

    #[test]
    fn a_failures_reason_is_the_last_lines_of_stderr() {
        let stderr = "\nfirst\n\nsecond\nthird\n";
        assert_eq!(tail(stderr, 2).as_deref(), Some("second\nthird"));
        assert_eq!(tail(stderr, 10).as_deref(), Some("first\nsecond\nthird"));
        assert_eq!(tail(" \n", 3), None);
    }
}

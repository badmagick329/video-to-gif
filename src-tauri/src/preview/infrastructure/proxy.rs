//! Running the ffmpeg that makes a proxy: streaming its progress and stopping it on demand.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, ChildStdout, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crate::preview::domain::{PreviewError, progress_from_line};
use crate::process;

/// A running proxy encode. Shared between the thread that waits on it and whoever may cancel it.
#[derive(Debug)]
pub struct ProxyJob {
    child: Mutex<Child>,
    cancelled: AtomicBool,
}

/// What [`ProxyJob::start`] hands to the waiting thread alongside the job.
pub struct ProxyOutput {
    stdout: ChildStdout,
    stderr: thread::JoinHandle<String>,
}

impl ProxyJob {
    /// Starts `ffmpeg` with `args`, which must write progress to stdout (`-progress pipe:1`).
    pub fn start(ffmpeg: &Path, args: &[OsString]) -> Result<(Self, ProxyOutput), PreviewError> {
        let mut child = process::command(ffmpeg)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| PreviewError::ProxyFailed {
                reason: format!("ffmpeg could not start: {err}"),
            })?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        // Drained on its own thread so a chatty ffmpeg never blocks on a full stderr pipe while
        // the caller reads stdout.
        let stderr = thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        });
        let job = Self {
            child: Mutex::new(child),
            cancelled: AtomicBool::new(false),
        };
        Ok((job, ProxyOutput { stdout, stderr }))
    }

    /// Reads progress until ffmpeg exits, calling `on_progress` with 0..=1 against the source's
    /// `duration`, and reports how the encode ended. On failure or cancellation the partial
    /// `output` is deleted.
    pub fn finish(
        &self,
        output: ProxyOutput,
        duration: f64,
        target: &Path,
        mut on_progress: impl FnMut(f64),
    ) -> Result<(), PreviewError> {
        for line in BufReader::new(output.stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(progress) = progress_from_line(&line, duration) {
                on_progress(progress);
            }
        }
        let status = self.wait();
        let stderr = output.stderr.join().unwrap_or_default();

        let result = if self.cancelled.load(Ordering::SeqCst) {
            Err(PreviewError::Cancelled)
        } else {
            match status {
                Ok(status) if status.success() => Ok(()),
                Ok(status) => Err(PreviewError::ProxyFailed {
                    reason: last_line(&stderr)
                        .unwrap_or_else(|| format!("ffmpeg exited with {status}")),
                }),
                Err(err) => Err(PreviewError::ProxyFailed {
                    reason: err.to_string(),
                }),
            }
        };
        if result.is_err() {
            let _ = std::fs::remove_file(target);
        }
        result
    }

    /// Stops the encode; the thread in [`Self::finish`] then returns [`PreviewError::Cancelled`].
    /// With `wait`, returns only once ffmpeg has exited, so its output file is no longer open.
    pub fn cancel(&self, wait: bool) {
        self.cancelled.store(true, Ordering::SeqCst);
        let mut child = self.lock_child();
        // Fails only if ffmpeg already exited, which is what cancelling wants anyway.
        let _ = child.kill();
        if wait {
            let _ = child.wait();
        }
    }

    /// Waits for ffmpeg to exit by polling, so the lock is free between polls and [`Self::cancel`]
    /// can kill it at any moment. stdout has already closed, so ffmpeg is at most finishing its
    /// output file.
    fn wait(&self) -> std::io::Result<std::process::ExitStatus> {
        loop {
            if let Some(status) = self.lock_child().try_wait()? {
                return Ok(status);
            }
            thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn lock_child(&self) -> std::sync::MutexGuard<'_, Child> {
        // A panic elsewhere while holding the lock leaves the Child itself intact.
        self.child
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

fn last_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_owned)
}

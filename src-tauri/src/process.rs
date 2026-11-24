//! Starting ffmpeg and ffprobe from a GUI app.
//!
//! Shared kernel: `probe`, `preview` and `export` all spawn these programs and must all hide the
//! console window, so the flags live here once rather than in each slice.

use std::path::Path;
use std::process::{Command, Stdio};

/// How hard a process may compete for the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Work the user is waiting on: probing, proxies, thumbnails, exports.
    Normal,
    /// Work the user did not ask for, such as size estimates: Windows runs it only on CPU time
    /// that normal-priority programs, the app's own player and exports included, leave over.
    Background,
}

/// A command for `program` that opens no console window and reads nothing from stdin.
///
/// The release build is a GUI-subsystem app with no console, so Windows would give each console
/// child its own window, flashing on screen. ffmpeg also reads stdin for interactive keys; a null
/// stdin keeps it from waiting on, or eating, input that is not meant for it.
pub fn command(program: &Path) -> Command {
    command_at(program, Priority::Normal)
}

/// [`command`], at `priority`. ffmpeg's own worker threads inherit the process's priority class.
pub fn command_at(program: &Path, priority: Priority) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        let priority = match priority {
            Priority::Normal => 0,
            Priority::Background => BELOW_NORMAL_PRIORITY_CLASS,
        };
        command.creation_flags(CREATE_NO_WINDOW | priority);
    }
    #[cfg(not(windows))]
    let _ = priority;
    command
}

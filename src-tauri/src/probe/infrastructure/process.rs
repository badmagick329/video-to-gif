//! Starting ffmpeg and ffprobe from a GUI app.

use std::path::Path;
use std::process::{Command, Stdio};

/// A command for `program` that opens no console window and reads nothing from stdin.
///
/// The release build is a GUI-subsystem app with no console, so Windows would give each console
/// child its own window, flashing on screen. ffmpeg also reads stdin for interactive keys; a null
/// stdin keeps it from waiting on, or eating, input that is not meant for it.
pub fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

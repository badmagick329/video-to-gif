//! Output files in the destination folder: a temporary name while ffmpeg writes, and a move to the
//! final name that never replaces an existing file.
//!
//! ffmpeg writes each output under a name no one else uses, and the file takes its real name only
//! once ffmpeg has exited successfully, so a cancelled or failed run never leaves a half-written
//! file under an output's name (gif-studio's defect, OSS-4 item 1). The temporary file is deleted
//! on failure; only a crash or power cut can leave one, named `<stem>.part-<pid>-<n>.<ext>`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::export::domain::plan::numbered_name;

/// Numbers temporary files within this process; with the pid it keeps two runs, or two app
/// windows, from picking the same temporary name.
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

/// How many numbered names to try before giving up on a folder full of `name (n).ext` files.
const MAX_NUMBER: u32 = 10_000;

/// A name in `dir` for ffmpeg to write `final_name`'s output to. It keeps the extension, from
/// which ffmpeg picks the muxer. ffmpeg runs with `-n`, so even a name taken after this check
/// fails the run rather than overwriting anything.
pub fn temp_path(dir: &Path, final_name: &str) -> PathBuf {
    let (stem, ext) = match final_name.rfind('.') {
        Some(dot) if dot > 0 => (&final_name[..dot], &final_name[dot..]),
        _ => (final_name, ""),
    };
    loop {
        let n = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let candidate = dir.join(format!("{stem}.part-{}-{n}{ext}", std::process::id()));
        if !candidate.exists() {
            return candidate;
        }
    }
}

/// Moves the finished `temp` file to `name` in `dir`, or to `name (1)`, `name (2)` and so on when
/// a name is taken, and returns the path used. The name is claimed by the move itself, which
/// fails rather than replacing a file, so two runs finishing at once cannot both take one name
/// and nothing already in the folder is overwritten.
pub fn publish(temp: &Path, dir: &Path, name: &str) -> io::Result<PathBuf> {
    for n in 0..=MAX_NUMBER {
        let target = dir.join(numbered_name(name, n));
        match move_no_replace(temp, &target) {
            Ok(()) => return Ok(target),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("{name} and {MAX_NUMBER} numbered names are all taken"),
    ))
}

/// Deletes a temporary file that will not be published. Missing is fine: ffmpeg may have failed
/// before creating it.
pub fn discard(temp: &Path) {
    let _ = fs::remove_file(temp);
}

/// Renames `from` to `to` within one folder, failing with [`io::ErrorKind::AlreadyExists`] when
/// `to` exists. Windows' `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` checks and renames in
/// one step, where `std::fs::rename` would replace the file.
#[cfg(windows)]
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain([0]).collect()
    }
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both pointers are to NUL-terminated UTF-16 strings that outlive the call.
    let moved = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) };
    if moved == 0 {
        // ERROR_FILE_EXISTS and ERROR_ALREADY_EXISTS map to ErrorKind::AlreadyExists.
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Elsewhere a hard link claims the name atomically (it fails if `to` exists), then the temporary
/// name goes.
#[cfg(not(windows))]
fn move_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::hard_link(from, to)?;
    fs::remove_file(from)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use tempfile::TempDir;

    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_temporary_name_keeps_the_extension_and_is_unused() {
        let dir = TempDir::new().unwrap();
        let first = temp_path(dir.path(), "dance.gif");
        fs::write(&first, b"").unwrap();
        let second = temp_path(dir.path(), "dance.gif");
        assert_ne!(first, second);
        let name = second.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("dance.part-"), "{name}");
        assert!(name.ends_with(".gif"), "{name}");
    }

    #[test]
    fn publishing_numbers_a_taken_name_and_overwrites_nothing() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("dance.gif"), b"mine").unwrap();
        fs::write(dir.path().join("dance (1).gif"), b"also mine").unwrap();
        let temp = temp_path(dir.path(), "dance.gif");
        fs::write(&temp, b"new").unwrap();

        let path = publish(&temp, dir.path(), "dance.gif").unwrap();

        assert_eq!(path, dir.path().join("dance (2).gif"));
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read(dir.path().join("dance.gif")).unwrap(), b"mine");
        assert_eq!(
            names(dir.path()),
            ["dance (1).gif", "dance (2).gif", "dance.gif"]
        );
    }

    #[test]
    fn two_runs_finishing_at_once_never_share_a_name() {
        let dir = TempDir::new().unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let dir = dir.path().to_owned();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    let temp = temp_path(&dir, "dance.gif");
                    fs::write(&temp, i.to_string()).unwrap();
                    barrier.wait();
                    publish(&temp, &dir, "dance.gif").unwrap()
                })
            })
            .collect();
        let mut paths: Vec<PathBuf> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 8);
        let mut contents: Vec<String> = paths
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect();
        contents.sort();
        assert_eq!(contents, (0..8).map(|i| i.to_string()).collect::<Vec<_>>());
        assert_eq!(names(dir.path()).len(), 8, "no temporary file is left");
    }
}

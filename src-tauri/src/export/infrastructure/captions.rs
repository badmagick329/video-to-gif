//! The caption image ffmpeg overlays: written to the session's temp folder for one export and
//! deleted when the export ends, however it ends.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Numbers caption files, so an export never writes over one an earlier export may still be
/// deleting.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// A caption PNG on disk; dropping it deletes the file.
#[derive(Debug)]
pub struct CaptionFile {
    path: PathBuf,
}

impl CaptionFile {
    /// Writes `png` into `dir`, creating `dir` if needed.
    pub fn write(dir: &Path, png: &[u8]) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(format!(
            "captions-{}.png",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, png)?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for CaptionFile {
    fn drop(&mut self) {
        // ffmpeg has exited by the time the export drops this, so nothing holds the file open.
        // A file that cannot be deleted goes with the session folder at exit.
        let _ = fs::remove_file(&self.path);
    }
}

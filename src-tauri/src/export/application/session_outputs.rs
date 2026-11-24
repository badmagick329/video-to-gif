//! The outputs this session has made, so Open, Show in folder and Copy act only on those.
//!
//! The paths come back from the webview, which is a trust boundary: without this check any script
//! in it could ask the shell to open an arbitrary file. A path is accepted only when it is, byte
//! for byte, one that `run_export` reported, and the file is still there.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::export::domain::outcome::OutputError;

/// Managed by Tauri for the app's lifetime; the set only grows.
#[derive(Debug, Default)]
pub struct SessionOutputs {
    made: Mutex<HashSet<PathBuf>>,
}

impl SessionOutputs {
    /// Records a finished output. Called before the frontend hears of it, so a click on its Open
    /// button can never race the record.
    pub fn record(&self, path: &Path) {
        self.lock().insert(path.to_path_buf());
    }

    /// The path, when it names an output this session made that still exists.
    pub fn check(&self, path: &str) -> Result<PathBuf, OutputError> {
        let path = PathBuf::from(path);
        if !self.lock().contains(&path) {
            return Err(OutputError::NotAnOutput {
                path: path.display().to_string(),
            });
        }
        if !path.is_file() {
            return Err(OutputError::Missing {
                path: path.display().to_string(),
            });
        }
        Ok(path)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<PathBuf>> {
        // A set of paths cannot be left half-updated, so a poisoned lock is still sound.
        self.made
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn output_in(dir: &TempDir, name: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, b"GIF89a").unwrap();
        path
    }

    #[test]
    fn accepts_an_output_this_session_made() {
        let dir = TempDir::new().unwrap();
        let gif = output_in(&dir, "clip.gif");
        let outputs = SessionOutputs::default();
        outputs.record(&gif);
        assert_eq!(outputs.check(&gif.display().to_string()), Ok(gif));
    }

    #[test]
    fn refuses_a_file_it_did_not_make() {
        let dir = TempDir::new().unwrap();
        let made = output_in(&dir, "clip.gif");
        let other = output_in(&dir, "notes.txt");
        let outputs = SessionOutputs::default();
        outputs.record(&made);
        for path in [
            other.display().to_string(),
            // The folder holding an output is not an output.
            dir.path().display().to_string(),
            "C:\\Windows\\System32\\cmd.exe".to_string(),
            "clip.gif".to_string(),
        ] {
            assert_eq!(
                outputs.check(&path),
                Err(OutputError::NotAnOutput { path: path.clone() }),
                "{path}"
            );
        }
    }

    #[test]
    fn refuses_an_output_that_has_since_gone() {
        let dir = TempDir::new().unwrap();
        let gif = output_in(&dir, "clip.gif");
        let outputs = SessionOutputs::default();
        outputs.record(&gif);
        fs::remove_file(&gif).unwrap();
        let path = gif.display().to_string();
        assert_eq!(outputs.check(&path), Err(OutputError::Missing { path }));
    }
}

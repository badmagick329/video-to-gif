//! The remembered destination: the folder the user picked with Save to, kept across launches as
//! `destination.json` in the app's config folder (`%APPDATA%\ink.mgck.video-to-gif` on Windows).
//!
//! The file is read back as untrusted: anyone can edit it, and the folder it names can be
//! deleted, renamed or on a drive that is gone. Loading returns a folder only when the file parses
//! and names an absolute path to an existing folder; anything else means "none", and the editor
//! falls back to the source's folder. A file naming a folder that is missing now is kept, so a
//! drive plugged back in brings the choice back on the next launch.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "destination.json";

#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    folder: PathBuf,
}

/// Where the choice is kept. Managed by Tauri for the app's lifetime.
#[derive(Debug)]
pub struct SavedDestination {
    file: PathBuf,
}

impl SavedDestination {
    /// Keeps the choice in `config_dir`, which need not exist yet.
    pub fn in_dir(config_dir: &Path) -> Self {
        Self {
            file: config_dir.join(FILE_NAME),
        }
    }

    /// The remembered folder, when there is one and it exists now.
    pub fn load(&self) -> Option<PathBuf> {
        let text = fs::read_to_string(&self.file).ok()?;
        let stored: Stored = serde_json::from_str(&text).ok()?;
        is_folder(&stored.folder).then_some(stored.folder)
    }

    /// Remembers `folder`, or forgets the choice with `None`. The caller has checked the folder.
    pub fn save(&self, folder: Option<&Path>) -> io::Result<()> {
        let Some(folder) = folder else {
            return match fs::remove_file(&self.file) {
                Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
                _ => Ok(()),
            };
        };
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&Stored {
            folder: folder.to_path_buf(),
        })
        .map_err(io::Error::other)?;
        fs::write(&self.file, text)
    }
}

/// An absolute path to an existing folder: what `run_export` accepts as a destination.
pub fn is_folder(path: &Path) -> bool {
    path.is_absolute() && path.is_dir()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    /// A config dir that does not exist yet, as on a first launch, and a folder to remember.
    fn setup() -> (TempDir, SavedDestination, PathBuf) {
        let root = TempDir::new().unwrap();
        let saved = SavedDestination::in_dir(&root.path().join("config"));
        let folder = root.path().join("gifs");
        fs::create_dir(&folder).unwrap();
        (root, saved, folder)
    }

    #[test]
    fn nothing_is_remembered_on_a_first_launch() {
        let (_root, saved, _) = setup();
        assert_eq!(saved.load(), None);
    }

    #[test]
    fn a_saved_folder_comes_back_on_the_next_launch() {
        let (root, saved, folder) = setup();
        saved.save(Some(&folder)).unwrap();
        // A new instance reads only the file, as a relaunched app does.
        let relaunched = SavedDestination::in_dir(&root.path().join("config"));
        assert_eq!(relaunched.load(), Some(folder));
    }

    #[test]
    fn forgetting_returns_to_none_and_tolerates_nothing_saved() {
        let (_root, saved, folder) = setup();
        saved.save(None).unwrap();
        saved.save(Some(&folder)).unwrap();
        saved.save(None).unwrap();
        assert_eq!(saved.load(), None);
    }

    #[test]
    fn a_folder_that_no_longer_exists_is_not_offered_until_it_returns() {
        let (_root, saved, folder) = setup();
        saved.save(Some(&folder)).unwrap();
        fs::remove_dir(&folder).unwrap();
        assert_eq!(saved.load(), None);
        // Kept on disk: a drive plugged back in brings the choice back.
        fs::create_dir(&folder).unwrap();
        assert_eq!(saved.load(), Some(folder));
    }

    #[test]
    fn a_hand_edited_or_corrupt_file_is_ignored() {
        let (root, saved, folder) = setup();
        let file = root.path().join("config").join(FILE_NAME);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let not_a_folder = folder.join("clip.gif");
        fs::write(&not_a_folder, b"GIF89a").unwrap();
        for text in [
            "not json".to_string(),
            "{}".to_string(),
            r#"{"folder": 3}"#.to_string(),
            r#"{"folder": "gifs"}"#.to_string(),
            serde_json::json!({ "folder": not_a_folder }).to_string(),
        ] {
            fs::write(&file, &text).unwrap();
            assert_eq!(saved.load(), None, "{text}");
        }
    }
}

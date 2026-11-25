//! Recent: the last sources opened, kept across launches as `recent.json` in the app's config
//! folder, beside the export slice's `destination.json` (`%APPDATA%\ink.mgck.video-to-gif` on
//! Windows).
//!
//! The file is read back as untrusted: anyone can edit it, and the files it names can be deleted
//! or moved. Loading keeps only entries with an absolute path to a file that exists now and a
//! positive duration, and writes the list back when it dropped any, so a deleted video leaves
//! Recent on the next load. A file that does not parse counts as an empty list.
//!
//! Recent only lists paths. Reopening one goes through `probe_source` and `open_source` like a
//! dialog pick, so the webview gains access to a file only when it is opened, never by being listed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::preview::domain::{RecentSource, remember};

const FILE_NAME: &str = "recent.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    sources: Vec<RecentSource>,
}

/// Where Recent is kept. Managed by Tauri for the app's lifetime.
#[derive(Debug)]
pub struct RecentSources {
    file: PathBuf,
    /// Serialises read-modify-write: commands run concurrently, and two opens racing would
    /// otherwise each write a list missing the other's source.
    lock: Mutex<()>,
}

impl RecentSources {
    /// Keeps Recent in `config_dir`, which need not exist yet.
    pub fn in_dir(config_dir: &Path) -> Self {
        Self {
            file: config_dir.join(FILE_NAME),
            lock: Mutex::new(()),
        }
    }

    /// Recent, newest first, without the sources whose files are gone. Writes the list back when
    /// it dropped any.
    pub fn load(&self) -> io::Result<Vec<RecentSource>> {
        let _guard = self.lock();
        let stored = self.read();
        let total = stored.len();
        let kept: Vec<RecentSource> = stored.into_iter().filter(is_usable).collect();
        if kept.len() != total {
            self.write(&kept)?;
        }
        Ok(kept)
    }

    /// Puts `opened` at the top of Recent.
    pub fn add(&self, opened: RecentSource) -> io::Result<()> {
        let _guard = self.lock();
        let mut sources = self.read();
        remember(&mut sources, opened);
        self.write(&sources)
    }

    /// Takes `path` off Recent; a path not on it is ignored.
    pub fn remove(&self, path: &Path) -> io::Result<()> {
        let _guard = self.lock();
        let mut sources = self.read();
        let total = sources.len();
        sources.retain(|source| source.path != path);
        if sources.len() == total {
            return Ok(());
        }
        self.write(&sources)
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// The stored list as written, or empty when there is none or it does not parse.
    fn read(&self) -> Vec<RecentSource> {
        fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str::<Stored>(&text).ok())
            .map(|stored| stored.sources)
            .unwrap_or_default()
    }

    fn write(&self, sources: &[RecentSource]) -> io::Result<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&Stored {
            sources: sources.to_vec(),
        })
        .map_err(io::Error::other)?;
        fs::write(&self.file, text)
    }
}

/// An entry worth offering: what `open_source` itself would accept, and still on disk.
fn is_usable(source: &RecentSource) -> bool {
    source.path.is_absolute()
        && source.path.is_file()
        && source.duration.is_finite()
        && source.duration > 0.0
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::preview::domain::RECENT_LIMIT;

    /// A config dir that does not exist yet, as on a first launch, and a folder of videos.
    fn setup() -> (TempDir, RecentSources) {
        let root = TempDir::new().unwrap();
        let recent = RecentSources::in_dir(&root.path().join("config"));
        fs::create_dir(root.path().join("videos")).unwrap();
        (root, recent)
    }

    /// A file at `videos/name` and its Recent entry.
    fn video(root: &TempDir, name: &str, opened_at: u64) -> RecentSource {
        let path = root.path().join("videos").join(name);
        fs::write(&path, b"not really a video").unwrap();
        RecentSource {
            path,
            duration: 12.5,
            opened_at,
        }
    }

    fn names(sources: &[RecentSource]) -> Vec<String> {
        sources
            .iter()
            .map(|source| {
                source
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn recent_is_empty_on_a_first_launch() {
        let (_root, recent) = setup();
        assert_eq!(recent.load().unwrap(), []);
    }

    #[test]
    fn opened_sources_come_back_newest_first_after_a_relaunch() {
        let (root, recent) = setup();
        let first = video(&root, "first.mp4", 1_000);
        let second = video(&root, "second.mkv", 2_000);
        recent.add(first.clone()).unwrap();
        recent.add(second.clone()).unwrap();
        // A new instance reads only the file, as a relaunched app does.
        let relaunched = RecentSources::in_dir(&root.path().join("config"));
        assert_eq!(relaunched.load().unwrap(), [second, first]);
    }

    #[test]
    fn opening_a_source_again_moves_it_to_the_top_once() {
        let (root, recent) = setup();
        let a = video(&root, "a.mp4", 1);
        recent.add(a.clone()).unwrap();
        recent.add(video(&root, "b.mp4", 2)).unwrap();
        recent.add(RecentSource { opened_at: 3, ..a }).unwrap();
        let loaded = recent.load().unwrap();
        assert_eq!(names(&loaded), ["a.mp4", "b.mp4"]);
        assert_eq!(loaded[0].opened_at, 3);
    }

    #[test]
    fn recent_keeps_the_last_eight() {
        let (root, recent) = setup();
        for n in 0..10 {
            recent.add(video(&root, &format!("{n}.mp4"), n)).unwrap();
        }
        let loaded = recent.load().unwrap();
        assert_eq!(loaded.len(), RECENT_LIMIT);
        assert_eq!(names(&loaded).first().unwrap(), "9.mp4");
        assert_eq!(names(&loaded).last().unwrap(), "2.mp4");
    }

    #[test]
    fn a_deleted_file_is_dropped_on_the_next_load_and_stays_dropped() {
        let (root, recent) = setup();
        let kept = video(&root, "kept.mp4", 1);
        let deleted = video(&root, "deleted.mp4", 2);
        recent.add(kept.clone()).unwrap();
        recent.add(deleted.clone()).unwrap();
        fs::remove_file(&deleted.path).unwrap();
        assert_eq!(recent.load().unwrap(), std::slice::from_ref(&kept));
        // Written back: the file reappearing does not bring the entry back.
        fs::write(&deleted.path, b"back").unwrap();
        assert_eq!(recent.load().unwrap(), [kept]);
    }

    #[test]
    fn remove_takes_one_entry_off_and_ignores_unknown_paths() {
        let (root, recent) = setup();
        let a = video(&root, "a.mp4", 1);
        let b = video(&root, "b.mp4", 2);
        recent.add(a.clone()).unwrap();
        recent.add(b.clone()).unwrap();
        recent.remove(&b.path).unwrap();
        recent
            .remove(&root.path().join("videos").join("never.mp4"))
            .unwrap();
        assert_eq!(recent.load().unwrap(), [a]);
        // The file itself is untouched.
        assert!(b.path.is_file());
    }

    #[test]
    fn a_hand_edited_or_corrupt_file_offers_only_usable_entries() {
        let (root, recent) = setup();
        let good = video(&root, "good.mp4", 1);
        let file = root.path().join("config").join(FILE_NAME);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        for text in ["not json", "{}", r#"{"sources": 3}"#] {
            fs::write(&file, text).unwrap();
            assert_eq!(recent.load().unwrap(), [], "{text}");
        }
        let bad = |path: &Path, duration: f64| serde_json::json!({ "path": path, "duration": duration, "openedAt": 1 });
        let text = serde_json::json!({
            "sources": [
                good,
                bad(Path::new("videos/relative.mp4"), 3.0),
                bad(&root.path().join("videos"), 3.0),
                bad(&good.path, 0.0),
            ]
        });
        fs::write(&file, text.to_string()).unwrap();
        assert_eq!(recent.load().unwrap(), [good]);
    }
}

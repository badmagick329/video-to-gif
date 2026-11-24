//! The app's temp folder, where proxies live: `<OS temp>/video-to-gif/session-<pid>/`.
//!
//! Each running app owns one session folder and holds an exclusive lock on a `lock` file inside
//! it for as long as it runs. At startup a session sweeps the root: any session folder whose lock
//! it can take belongs to an app that crashed or was killed, so it is deleted; locked folders
//! belong to another running copy of the app and are left alone. The lock, not the pid, decides,
//! because pids are reused and a lock dies with its process.

use std::fs::{self, File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

/// Name of the folder under the OS temp dir that holds every session.
pub const ROOT_NAME: &str = "video-to-gif";
const LOCK_FILE: &str = "lock";
const SESSION_PREFIX: &str = "session-";

/// This process's session folder, swept and locked at creation and deleted by [`Self::remove`].
#[derive(Debug)]
pub struct TempFolder {
    root: PathBuf,
    session: PathBuf,
    /// Held open and locked for the session's life; `None` once removed.
    lock: Option<File>,
}

impl TempFolder {
    /// The default root under the OS temp dir.
    pub fn default_root() -> PathBuf {
        std::env::temp_dir().join(ROOT_NAME)
    }

    /// Sweeps `root` of what crashed sessions left behind, then creates and locks this process's
    /// session folder in it.
    pub fn create(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        sweep(root);
        let session = root.join(format!("{SESSION_PREFIX}{}", std::process::id()));
        fs::create_dir_all(&session)?;
        let lock = File::create(session.join(LOCK_FILE))?;
        lock.try_lock().map_err(|err| match err {
            TryLockError::Error(err) => err,
            TryLockError::WouldBlock => io::Error::other("session folder is locked by another app"),
        })?;
        Ok(Self {
            root: root.to_owned(),
            session,
            lock: Some(lock),
        })
    }

    /// The folder this session writes proxies to.
    pub fn session_dir(&self) -> &Path {
        &self.session
    }

    /// Deletes the session folder and everything in it, then the root if no other session is
    /// using it. Callers stop anything writing into the folder first: on Windows a file still
    /// open in another process cannot be deleted. Idempotent.
    pub fn remove(&mut self) -> io::Result<()> {
        if self.lock.take().is_none() {
            return Ok(());
        }
        remove_dir_if_present(&self.session)?;
        // Fails, harmlessly, while another session's folder is still in the root.
        let _ = fs::remove_dir(&self.root);
        Ok(())
    }
}

/// Deletes every entry in `root` except session folders whose lock is held. Best effort: an entry
/// that cannot be deleted now is retried by the next session's sweep.
fn sweep(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_session_dir = entry.file_type().is_ok_and(|t| t.is_dir())
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(SESSION_PREFIX));
        if is_session_dir {
            if !is_live(&path) {
                let _ = fs::remove_dir_all(&path);
            }
        } else if entry.file_type().is_ok_and(|t| t.is_dir()) {
            let _ = fs::remove_dir_all(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Whether another running app holds this session folder's lock.
fn is_live(session: &Path) -> bool {
    let Ok(lock) = File::open(session.join(LOCK_FILE)) else {
        // No lock file: a session that died before taking it, or one already half-deleted.
        return false;
    };
    matches!(lock.try_lock(), Err(TryLockError::WouldBlock))
}

fn remove_dir_if_present(dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(dir) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// A folder as a crashed session leaves it: an unlocked lock file and a proxy.
    fn crashed_session(root: &Path, pid: u32) -> PathBuf {
        let dir = root.join(format!("{SESSION_PREFIX}{pid}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(LOCK_FILE), b"").unwrap();
        fs::write(dir.join("proxy-1.mp4"), b"left behind").unwrap();
        dir
    }

    #[test]
    fn startup_sweeps_what_a_crashed_session_left() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join(ROOT_NAME);
        let crashed = crashed_session(&root, 1);
        fs::write(root.join("stray.mp4"), b"").unwrap();

        let folder = TempFolder::create(&root).unwrap();

        assert!(!crashed.exists());
        assert_eq!(
            entries(&root),
            vec![format!("{SESSION_PREFIX}{}", std::process::id())]
        );
        assert_eq!(entries(folder.session_dir()), vec![LOCK_FILE]);
    }

    #[test]
    fn startup_leaves_a_running_session_alone() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join(ROOT_NAME);
        // Another running app: its own folder, its lock held by an open handle.
        let live = root.join(format!("{SESSION_PREFIX}1"));
        fs::create_dir_all(&live).unwrap();
        let held = File::create(live.join(LOCK_FILE)).unwrap();
        held.lock().unwrap();
        fs::write(live.join("proxy-1.mp4"), b"in use").unwrap();

        let mut folder = TempFolder::create(&root).unwrap();

        assert!(live.join("proxy-1.mp4").exists());
        folder.remove().unwrap();
        assert!(live.exists(), "the other session's folder must survive");
        assert!(root.exists(), "the root still holds the other session");
    }

    #[test]
    fn remove_leaves_no_temp_folder_behind() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join(ROOT_NAME);
        let mut folder = TempFolder::create(&root).unwrap();
        fs::write(folder.session_dir().join("proxy-1.mp4"), b"proxy").unwrap();

        folder.remove().unwrap();
        folder.remove().unwrap();

        assert!(!root.exists());
    }
}

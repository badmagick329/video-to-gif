//! Finding programs on PATH the way Windows does, so the app runs the same ffmpeg that
//! `where.exe ffmpeg` reports.
//!
//! Hand-rolled rather than relying on `Command::new("ffmpeg")`'s own search because the app needs
//! the resolved path up front: to report it, to look for `ffprobe` in the same folder, and to show
//! the setup message when nothing is found. The owner's PATH holds a folder named `ffmpeg` beside
//! `ffmpeg.exe` in its first ffmpeg directory; only regular files count as matches, so that folder
//! never shadows anything.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// The directories and file extensions to search, read from `PATH` and `PATHEXT`.
#[derive(Debug, Clone)]
pub struct SearchPath {
    dirs: Vec<PathBuf>,
    /// Suffixes tried after a bare program name, in order. On Windows these come from PATHEXT
    /// (`.COM;.EXE;...`); elsewhere the bare name is the only candidate.
    extensions: Vec<OsString>,
}

/// Where `ffmpeg` and `ffprobe` were found, if anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedTools {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
}

impl SearchPath {
    /// The current process's `PATH` and `PATHEXT`.
    pub fn from_env() -> Self {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let pathext = std::env::var_os("PATHEXT");
        Self::new(&path, pathext.as_deref())
    }

    /// A search over an explicit `PATH` value; `pathext` is ignored off Windows. Windows' own
    /// default applies when `PATHEXT` is unset.
    pub fn new(path: &OsStr, pathext: Option<&OsStr>) -> Self {
        let dirs = std::env::split_paths(path)
            .filter(|dir| !dir.as_os_str().is_empty())
            .collect();
        Self {
            dirs,
            extensions: extensions(pathext),
        }
    }

    /// The first regular file named `name` plus one of the extensions, scanning directories in
    /// PATH order and, within a directory, extensions in PATHEXT order (as `where.exe` does).
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        self.dirs.iter().find_map(|dir| self.find_in(dir, name))
    }

    /// The program in one directory only.
    pub fn find_in(&self, dir: &Path, name: &str) -> Option<PathBuf> {
        self.extensions.iter().find_map(|ext| {
            let mut file_name = OsString::from(name);
            file_name.push(ext);
            let candidate = dir.join(file_name);
            candidate.is_file().then_some(candidate)
        })
    }

    /// Resolves `ffmpeg`, then looks for `ffprobe` beside it before searching PATH, so both come
    /// from one build whenever that build ships both. A PATH with two ffmpeg installs could
    /// otherwise pair one build's ffmpeg with another's ffprobe.
    pub fn locate_tools(&self) -> LocatedTools {
        let ffmpeg = self.find("ffmpeg");
        let beside_ffmpeg = ffmpeg
            .as_deref()
            .and_then(Path::parent)
            .and_then(|dir| self.find_in(dir, "ffprobe"));
        let ffprobe = beside_ffmpeg.or_else(|| self.find("ffprobe"));
        LocatedTools { ffmpeg, ffprobe }
    }
}

#[cfg(windows)]
fn extensions(pathext: Option<&OsStr>) -> Vec<OsString> {
    let pathext = pathext
        .and_then(OsStr::to_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(".COM;.EXE;.BAT;.CMD");
    pathext
        .split(';')
        .map(str::trim)
        .filter(|ext| ext.starts_with('.') && ext.len() > 1)
        // NTFS matches names case-insensitively, so case only affects the reported path;
        // lowercase gives the familiar `ffmpeg.exe` rather than PATHEXT's `ffmpeg.EXE`.
        .map(|ext| OsString::from(ext.to_ascii_lowercase()))
        .collect()
}

#[cfg(not(windows))]
fn extensions(_pathext: Option<&OsStr>) -> Vec<OsString> {
    vec![OsString::new()]
}

#[cfg(all(test, windows))]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    const PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

    fn search(dirs: &[&TempDir]) -> SearchPath {
        let path = std::env::join_paths(dirs.iter().map(|dir| dir.path())).unwrap();
        SearchPath::new(&path, Some(OsStr::new(PATHEXT)))
    }

    fn touch(dir: &TempDir, name: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn nothing_on_path_finds_neither_tool() {
        let empty = TempDir::new().unwrap();
        assert_eq!(
            search(&[&empty]).locate_tools(),
            LocatedTools {
                ffmpeg: None,
                ffprobe: None
            }
        );
    }

    #[test]
    fn an_empty_path_finds_neither_tool() {
        let tools = SearchPath::new(OsStr::new(""), Some(OsStr::new(PATHEXT))).locate_tools();
        assert_eq!(tools.ffmpeg, None);
        assert_eq!(tools.ffprobe, None);
    }

    #[test]
    fn a_folder_named_ffmpeg_does_not_shadow_the_exe_beside_it() {
        // The owner's C:\scripts: a folder `ffmpeg` next to `ffmpeg.exe`, ahead of another
        // install on PATH. Git Bash's `which` skips to the second install; Windows does not.
        let scripts = TempDir::new().unwrap();
        fs::create_dir(scripts.path().join("ffmpeg")).unwrap();
        let first = touch(&scripts, "ffmpeg.exe");
        let winget = TempDir::new().unwrap();
        touch(&winget, "ffmpeg.exe");

        assert_eq!(search(&[&scripts, &winget]).find("ffmpeg"), Some(first));
    }

    #[test]
    fn directories_never_match_even_with_an_exe_suffix() {
        let decoy = TempDir::new().unwrap();
        fs::create_dir(decoy.path().join("ffmpeg")).unwrap();
        fs::create_dir(decoy.path().join("ffmpeg.exe")).unwrap();
        let real = TempDir::new().unwrap();
        let exe = touch(&real, "ffmpeg.exe");

        assert_eq!(search(&[&decoy, &real]).find("ffmpeg"), Some(exe));
    }

    #[test]
    fn only_a_folder_named_ffmpeg_means_missing() {
        let decoy = TempDir::new().unwrap();
        fs::create_dir(decoy.path().join("ffmpeg")).unwrap();
        assert_eq!(search(&[&decoy]).find("ffmpeg"), None);
    }

    #[test]
    fn pathext_order_decides_within_a_directory() {
        let dir = TempDir::new().unwrap();
        let com = touch(&dir, "ffmpeg.com");
        touch(&dir, "ffmpeg.exe");
        assert_eq!(search(&[&dir]).find("ffmpeg"), Some(com));
    }

    #[test]
    fn ffprobe_beside_ffmpeg_wins_over_an_earlier_path_entry() {
        let earlier = TempDir::new().unwrap();
        touch(&earlier, "ffprobe.exe");
        let build = TempDir::new().unwrap();
        let ffmpeg = touch(&build, "ffmpeg.exe");
        let ffprobe = touch(&build, "ffprobe.exe");

        assert_eq!(
            search(&[&earlier, &build]).locate_tools(),
            LocatedTools {
                ffmpeg: Some(ffmpeg),
                ffprobe: Some(ffprobe)
            }
        );
    }

    #[test]
    fn ffprobe_falls_back_to_path_when_not_beside_ffmpeg() {
        let build = TempDir::new().unwrap();
        let ffmpeg = touch(&build, "ffmpeg.exe");
        let other = TempDir::new().unwrap();
        let ffprobe = touch(&other, "ffprobe.exe");

        let tools = search(&[&build, &other]).locate_tools();
        assert_eq!(tools.ffmpeg, Some(ffmpeg));
        assert_eq!(tools.ffprobe, Some(ffprobe));
    }

    #[test]
    fn ffprobe_alone_is_found_with_ffmpeg_missing() {
        let dir = TempDir::new().unwrap();
        let ffprobe = touch(&dir, "ffprobe.exe");
        let tools = search(&[&dir]).locate_tools();
        assert_eq!(tools.ffmpeg, None);
        assert_eq!(tools.ffprobe, Some(ffprobe));
    }
}

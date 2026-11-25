//! Opening a source and Recent, through `open_source`'s body with a temp app-data folder and a
//! short clip that the real ffmpeg on PATH makes at test time. A "relaunch" is a new
//! [`Previewer`] and [`RecentSources`] over the same app-data folder, as a restarted app builds.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::open;
use crate::preview::domain::{PreviewError, PreviewSource};
use crate::preview::infrastructure::recent::RecentSources;
use crate::preview::infrastructure::session::Previewer;
use crate::probe::domain::SourceInfo;
use crate::probe::infrastructure::ffprobe;
use crate::process;
use crate::search_path::SearchPath;
use crate::temp_folder::TempFolder;

/// One app's run: its preview state and Recent, kept in `app_data`.
struct Launch {
    previewer: Previewer,
    recent: RecentSources,
    /// The paths `open` gave the webview access to.
    admitted: RefCell<Vec<PathBuf>>,
}

impl Launch {
    fn new(app_data: &Path, temp_root: &Path) -> Self {
        Self {
            previewer: Previewer::new(TempFolder::create(temp_root).unwrap()),
            recent: RecentSources::in_dir(app_data),
            admitted: RefCell::default(),
        }
    }

    /// Probes `path` and opens it, as the editor does for a dialog pick, a drop or Recent.
    fn open(&self, path: &Path, opened_at: u64) -> Result<PreviewSource, PreviewError> {
        let info = probe(path);
        self.open_with(path, info, opened_at, Ok(()))
    }

    fn open_with(
        &self,
        path: &Path,
        info: SourceInfo,
        opened_at: u64,
        admit: Result<(), String>,
    ) -> Result<PreviewSource, PreviewError> {
        open(
            &self.previewer,
            &self.recent,
            path.display().to_string(),
            info,
            opened_at,
            |file| {
                self.admitted.borrow_mut().push(file.to_owned());
                admit
            },
        )
    }
}

fn probe(path: &Path) -> SourceInfo {
    let ffprobe = SearchPath::from_env()
        .locate_tools()
        .ffprobe
        .expect("ffprobe must be on PATH for the open tests");
    ffprobe::probe(&ffprobe, path).unwrap()
}

/// A temp folder holding the app data, the preview temp folders and a 2 s H.264 clip.
struct Fixture {
    root: TempDir,
    clip: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        fs::create_dir(root.path().join("videos")).unwrap();
        let clip = root.path().join("videos").join("short clip.mp4");
        let ffmpeg = SearchPath::from_env()
            .locate_tools()
            .ffmpeg
            .expect("ffmpeg must be on PATH for the open tests");
        let output = process::command(&ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi"])
            .args(["-i", "testsrc2=size=320x180:rate=30:duration=2"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(&clip)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self { root, clip }
    }

    fn app_data(&self) -> PathBuf {
        self.root.path().join("app-data")
    }

    /// Launch number `n`, each with its own preview temp folder, all sharing the app data.
    fn launch(&self, n: u32) -> Launch {
        Launch::new(
            &self.app_data(),
            &self.root.path().join(format!("temp-{n}")),
        )
    }
}

#[test]
fn an_opened_video_is_in_recent_after_a_relaunch_and_reopens_through_the_same_route() {
    let fixture = Fixture::new();
    let first = fixture.launch(1);
    assert_eq!(first.recent.load().unwrap(), []);
    assert_eq!(first.open(&fixture.clip, 1_000), Ok(PreviewSource::Source));
    assert_eq!(
        *first.admitted.borrow(),
        std::slice::from_ref(&fixture.clip)
    );
    drop(first);

    let second = fixture.launch(2);
    let recent = second.recent.load().unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].path, fixture.clip);
    assert!(
        (recent[0].duration - 2.0).abs() < 0.1,
        "{}",
        recent[0].duration
    );
    assert_eq!(recent[0].opened_at, 1_000);
    // Listing admits nothing; only reopening does, like a dialog pick.
    assert!(second.admitted.borrow().is_empty());

    assert_eq!(
        second.open(&recent[0].path, 2_000),
        Ok(PreviewSource::Source)
    );
    assert_eq!(
        *second.admitted.borrow(),
        std::slice::from_ref(&fixture.clip)
    );
    assert_eq!(second.recent.load().unwrap()[0].opened_at, 2_000);
}

#[test]
fn a_deleted_video_leaves_recent_on_the_next_launch() {
    let fixture = Fixture::new();
    let first = fixture.launch(1);
    first.open(&fixture.clip, 1_000).unwrap();
    drop(first);

    fs::remove_file(&fixture.clip).unwrap();
    let second = fixture.launch(2);
    assert_eq!(second.recent.load().unwrap(), []);
    // Opening the old path fails as a missing file, admitting nothing.
    let info = SourceInfo {
        duration: 2.0,
        width: 320,
        height: 180,
        frame_rate: 30.0,
        video_codec: "h264".into(),
        container: "mov,mp4,m4a,3gp,3g2,mj2".into(),
        audio_codec: None,
    };
    let result = second.open_with(&fixture.clip, info, 2_000, Ok(()));
    assert!(
        matches!(result, Err(PreviewError::NotFound { .. })),
        "{result:?}"
    );
    assert!(second.admitted.borrow().is_empty());
}

#[test]
fn a_source_the_webview_cannot_be_given_stays_out_of_recent() {
    let fixture = Fixture::new();
    let launch = fixture.launch(1);
    let info = probe(&fixture.clip);
    let result = launch.open_with(&fixture.clip, info, 1_000, Err("scope refused".into()));
    assert!(
        matches!(&result, Err(PreviewError::NotServable { reason, .. }) if reason == "scope refused"),
        "{result:?}"
    );
    assert_eq!(launch.recent.load().unwrap(), []);
}

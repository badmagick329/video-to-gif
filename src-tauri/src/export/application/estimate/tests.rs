//! Size estimates with the real ffmpeg on PATH, against real exports of a clip generated at test
//! time: exact where the method encodes the whole selection, close where it samples, nothing left
//! in the temp folder, and a cancel stopping every sample.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;
use crate::export::application::{ExportControl, run};
use crate::export::domain::outcome::ExportOutput;
use crate::export::domain::plan::ExportRequest;
use crate::process;
use crate::search_path::SearchPath;

/// gifski on PATH, for the gifski GIF tests; looked up once.
fn gifski() -> Option<&'static Path> {
    static GIFSKI: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    GIFSKI
        .get_or_init(|| SearchPath::from_env().find("gifski"))
        .as_deref()
}

fn ffmpeg() -> PathBuf {
    SearchPath::from_env()
        .locate_tools()
        .ffmpeg
        .expect("ffmpeg must be on PATH for the estimate tests")
}

/// 320×240 `testsrc2` at 25 fps for 24 s with a tone: moving footage, long enough to be sampled
/// by one shared run (up to 20 s) or by a run per file type (past it).
fn clip(dir: &Path) -> PathBuf {
    let out = dir.join("clip.mp4");
    let output = process::command(&ffmpeg())
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc2=size=320x240:rate=25:duration=24")
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=24"])
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ])
        .args(["-c:a", "aac", "-shortest"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    out
}

fn request(path: &Path, patch: Value) -> ExportRequest {
    let mut value = json!({
        "path": path,
        "info": {
            "duration": 24.0, "width": 320, "height": 240, "frameRate": 25.0,
            "videoCodec": "h264", "container": "mov,mp4,m4a,3gp,3g2,mj2", "audioCodec": "aac"
        },
        "settings": {
            "width": 240, "fps": 15, "speed": 1, "direction": "forward", "rotation": 0,
            "flipHorizontal": false, "flipVertical": false, "fileTypes": ["gif", "webp", "mp4"],
            "gifColors": 256, "gifDither": "sierra2_4a", "plays": 0, "webpQuality": 80
        },
        "keepSound": true
    });
    for (key, field) in patch.as_object().unwrap() {
        if key == "settings" {
            for (name, setting) in field.as_object().unwrap() {
                value["settings"][name] = setting.clone();
            }
        } else {
            value[key] = field.clone();
        }
    }
    serde_json::from_value(value).unwrap()
}

/// The estimate and the export of the same request, by file type.
struct Compared {
    estimates: Vec<SizeEstimate>,
    sent: Vec<SizeEstimate>,
    exported: Vec<(FileType, u64)>,
    /// The estimate's temp folder after it returned.
    left_behind: Vec<PathBuf>,
}

impl Compared {
    fn estimated(&self, file_type: FileType) -> u64 {
        match self
            .estimates
            .iter()
            .find(|e| matches!(e, SizeEstimate::Done { file_type: t, .. } if *t == file_type))
        {
            Some(SizeEstimate::Done { bytes, .. }) => *bytes,
            _ => panic!("no {file_type:?} estimate: {:?}", self.estimates),
        }
    }

    fn exported(&self, file_type: FileType) -> u64 {
        self.exported
            .iter()
            .find(|(t, _)| *t == file_type)
            .map(|(_, size)| *size)
            .unwrap()
    }

    fn error(&self, file_type: FileType) -> f64 {
        let estimated = self.estimated(file_type) as f64;
        let exported = self.exported(file_type) as f64;
        (estimated - exported) / exported
    }
}

fn compare(patch: Value) -> Compared {
    let source_dir = TempDir::new().unwrap();
    let source = clip(source_dir.path());
    let work = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let ffmpeg = ffmpeg();
    let env = Environment {
        ffmpeg: &ffmpeg,
        gifski: gifski(),
        work_dir: work.path(),
        available_memory: None,
    };

    let plan = request(&source, patch.clone())
        .validate(work.path().to_owned())
        .unwrap();
    let sent = Mutex::new(Vec::new());
    let estimates = estimate(&plan, env, &EstimateControl::default(), &|e| {
        sent.lock().unwrap().push(e.clone())
    });
    let left_behind = fs::read_dir(work.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();

    let plan = request(&source, patch)
        .validate(destination.path().to_owned())
        .unwrap();
    let outputs = run(&plan, env, &ExportControl::default(), |_| {}, |_| {}).unwrap();
    let exported = outputs
        .into_iter()
        .map(|output| match output {
            ExportOutput::Done {
                file_type, size, ..
            } => (file_type, size),
            other => panic!("export failed: {other:?}"),
        })
        .collect();
    Compared {
        estimates,
        sent: sent.into_inner().unwrap(),
        exported,
        left_behind,
    }
}

#[test]
fn sampled_estimates_land_near_the_export_and_mp4_is_exact() {
    // A 7 s selection shares one run; a 22 s one runs each file type on its own.
    for (start, end) in [(0.5, 7.5), (1.0, 23.0)] {
        let compared = compare(json!({ "trim": { "start": start, "end": end } }));
        assert_eq!(
            compared.estimated(FileType::Mp4),
            compared.exported(FileType::Mp4)
        );
        for file_type in [FileType::Gif, FileType::Webp] {
            let error = compared.error(file_type);
            assert!(
                error.abs() < 0.15,
                "{start}-{end} s: {file_type:?} off by {error:.3}"
            );
        }
        assert_eq!(compared.left_behind, Vec::<PathBuf>::new());
    }
    let compared = compare(json!({ "trim": { "start": 0.5, "end": 7.5 } }));
    // Each file type was sent as it settled; the return value lists them in run order.
    assert_eq!(compared.sent.len(), 3);
    let order: Vec<FileType> = compared
        .estimates
        .iter()
        .map(|e| match e {
            SizeEstimate::Done { file_type, .. }
            | SizeEstimate::Failed { file_type, .. }
            | SizeEstimate::Cancelled { file_type } => *file_type,
        })
        .collect();
    assert_eq!(order, vec![FileType::Gif, FileType::Webp, FileType::Mp4]);
    assert_eq!(compared.left_behind, Vec::<PathBuf>::new());
}

#[test]
fn reverse_boomerang_crop_speed_and_captions_reach_the_samples() {
    // A caption PNG at the output size, 240×180: the samples must overlay it like the export.
    let png = caption_png(240, 180);
    for direction in ["reverse", "boomerang"] {
        let compared = compare(json!({
            "trim": { "start": 0.0, "end": 8.0 },
            "crop": { "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 },
            "settings": { "direction": direction, "speed": 1.25 },
            "captions": png,
        }));
        assert_eq!(
            compared.estimated(FileType::Mp4),
            compared.exported(FileType::Mp4)
        );
        for file_type in [FileType::Gif, FileType::Webp] {
            let error = compared.error(file_type);
            assert!(
                error.abs() < 0.15,
                "{direction} {file_type:?} off by {error:.3}"
            );
        }
        assert_eq!(compared.left_behind, Vec::<PathBuf>::new());
    }
}

#[test]
fn a_selection_no_longer_than_the_samples_is_estimated_exactly() {
    let compared = compare(json!({ "trim": { "start": 2.0, "end": 4.5 } }));
    for file_type in [FileType::Gif, FileType::Webp, FileType::Mp4] {
        assert_eq!(
            compared.estimated(file_type),
            compared.exported(file_type),
            "{file_type:?}"
        );
    }
}

#[test]
fn a_cancelled_estimate_stops_its_samples_and_leaves_nothing() {
    let source_dir = TempDir::new().unwrap();
    let source = clip(source_dir.path());
    let work = TempDir::new().unwrap();
    let ffmpeg = ffmpeg();
    let plan = request(&source, json!({}))
        .validate(work.path().to_owned())
        .unwrap();
    let estimates = Estimates::default();
    let control = estimates.begin();
    // A newer estimate cancels this one; the cancel lands before any sample starts here, and
    // `attach` kills a sample that starts after it.
    let newer = estimates.begin();
    let results = estimate(
        &plan,
        Environment {
            ffmpeg: &ffmpeg,
            gifski: gifski(),
            work_dir: work.path(),
            available_memory: None,
        },
        &control,
        &|_| {},
    );
    assert!(
        results
            .iter()
            .all(|e| matches!(e, SizeEstimate::Cancelled { .. })),
        "{results:?}"
    );
    assert!(!newer.is_cancelled());
    assert_eq!(fs::read_dir(work.path()).unwrap().count(), 0);
}

#[test]
fn a_whole_encode_that_would_not_fit_in_memory_is_not_run() {
    let source_dir = TempDir::new().unwrap();
    let source = clip(source_dir.path());
    let work = TempDir::new().unwrap();
    let ffmpeg = ffmpeg();
    let plan = request(
        &source,
        json!({ "settings": { "direction": "reverse", "fileTypes": ["mp4"] } }),
    )
    .validate(work.path().to_owned())
    .unwrap();
    let results = estimate(
        &plan,
        Environment {
            ffmpeg: &ffmpeg,
            gifski: gifski(),
            work_dir: work.path(),
            available_memory: Some(1024),
        },
        &EstimateControl::default(),
        &|_| {},
    );
    assert!(
        matches!(
            &results[..],
            [SizeEstimate::Failed {
                file_type: FileType::Mp4,
                ..
            }]
        ),
        "{results:?}"
    );
}

/// A transparent PNG of `width`×`height` with a white bar, made by ffmpeg.
fn caption_png(width: u32, height: u32) -> Vec<u8> {
    let dir = TempDir::new().unwrap();
    let out = dir.path().join("captions.png");
    let output = process::command(&ffmpeg())
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!(
            "color=c=black@0.0:size={width}x{height},format=rgba,drawbox=x=10:y=10:w=200:h=20:color=white:t=fill"
        ))
        .args(["-frames:v", "1"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    fs::read(out).unwrap()
}

//! The size estimate's accuracy and speed on real footage: for each clip named in
//! `VTG_ESTIMATE_CLIPS` (paths separated by `|`), estimates GIF, WebP and MP4 for 5 s and 20 s
//! selections at 480 px/15 fps and 640 px/24 fps, plus one Reverse and one Boomerang case on the
//! first clip, then exports each and prints estimate against export with the estimate's wall time.
//! Run it by hand, ffmpeg and ffprobe on PATH:
//!
//! ```text
//! VTG_ESTIMATE_CLIPS="C:\a.mp4|C:\b.mov" cargo test --release estimate_accuracy -- --ignored --nocapture
//! ```
//!
//! It lives in the shell, beside `lib.rs`, because it needs two slices: `probe` reads each clip's
//! source info the way the editor does, and `export` estimates and exports.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use serde_json::json;
use tempfile::TempDir;

use crate::export::application::estimate::{EstimateControl, estimate};
use crate::export::application::{Environment, ExportControl, run};
use crate::export::domain::estimate::SizeEstimate;
use crate::export::domain::outcome::ExportOutput;
use crate::export::domain::plan::ExportRequest;
use crate::export::domain::settings::FileType;
use crate::export::infrastructure::memory;
use crate::probe::domain::SourceInfo;
use crate::probe::infrastructure::ffprobe;
use crate::search_path::SearchPath;

struct Case {
    start: f64,
    end: f64,
    width: u32,
    fps: u32,
    direction: &'static str,
}

#[test]
#[ignore = "needs real clips in VTG_ESTIMATE_CLIPS; run by hand"]
fn estimate_accuracy() {
    let clips = std::env::var("VTG_ESTIMATE_CLIPS").expect("set VTG_ESTIMATE_CLIPS");
    let tools = SearchPath::from_env().locate_tools();
    let ffmpeg = tools.ffmpeg.expect("ffmpeg on PATH");
    let ffprobe_path = tools.ffprobe.expect("ffprobe on PATH");
    println!("clip | selection | settings | type | estimate | export | error | settled after s");
    for (index, clip) in clips.split('|').map(PathBuf::from).enumerate() {
        let info = ffprobe::probe(&ffprobe_path, &clip).expect("clip probes");
        for case in cases(&info, index == 0) {
            compare(&ffmpeg, &clip, &info, &case);
        }
    }
}

/// 5 s and 20 s selections starting 30% in (or the whole clip, when shorter), at two settings;
/// Reverse and Boomerang at 480/15 on 5 s.
fn cases(info: &SourceInfo, with_directions: bool) -> Vec<Case> {
    let mut cases = Vec::new();
    for length in [5.0, 20.0] {
        let start = (info.duration * 0.3).min((info.duration - length).max(0.0));
        let end = (start + length).min(info.duration);
        for (width, fps) in [(480, 15), (640, 24)] {
            cases.push(Case {
                start,
                end,
                width,
                fps,
                direction: "forward",
            });
        }
        if length > info.duration {
            break;
        }
    }
    if with_directions {
        let start = info.duration * 0.3;
        for direction in ["reverse", "boomerang"] {
            cases.push(Case {
                start,
                end: (start + 5.0).min(info.duration),
                width: 480,
                fps: 15,
                direction,
            });
        }
    }
    cases
}

fn request(clip: &Path, info: &SourceInfo, case: &Case) -> ExportRequest {
    let file_types: Vec<String> = std::env::var("VTG_ESTIMATE_TYPES")
        .unwrap_or_else(|_| "gif,webp,mp4".to_owned())
        .split(',')
        .map(str::to_owned)
        .collect();
    serde_json::from_value(json!({
        "path": clip,
        "info": info,
        "trim": { "start": case.start, "end": case.end },
        "settings": {
            "width": case.width, "fps": case.fps, "speed": 1, "direction": case.direction,
            "rotation": 0, "flipHorizontal": false, "flipVertical": false,
            "fileTypes": file_types, "gifColors": 256, "gifDither": "sierra2_4a",
            "plays": 0, "webpQuality": 80
        },
        "keepSound": false
    }))
    .unwrap()
}

fn compare(ffmpeg: &Path, clip: &Path, info: &SourceInfo, case: &Case) {
    let gifski = SearchPath::from_env().find("gifski");
    let work = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let env = Environment {
        ffmpeg,
        gifski: gifski.as_deref(),
        work_dir: work.path(),
        available_memory: memory::available_bytes(),
    };
    let plan = request(clip, info, case)
        .validate(work.path().to_owned())
        .unwrap();
    let started = Instant::now();
    let settled = Mutex::new(Vec::new());
    let estimates = estimate(&plan, env, &EstimateControl::default(), &|size| {
        let file_type = match size {
            SizeEstimate::Done { file_type, .. }
            | SizeEstimate::Failed { file_type, .. }
            | SizeEstimate::Cancelled { file_type } => *file_type,
        };
        settled
            .lock()
            .unwrap()
            .push((file_type, started.elapsed().as_secs_f64()));
    });
    let settled = settled.into_inner().unwrap();
    let wall = |file_type: FileType| {
        settled
            .iter()
            .find(|(t, _)| *t == file_type)
            .map_or(f64::NAN, |(_, seconds)| *seconds)
    };

    let plan = request(clip, info, case)
        .validate(destination.path().to_owned())
        .unwrap();
    let outputs = if std::env::var_os("VTG_ESTIMATE_NO_EXPORT").is_some() {
        Vec::new()
    } else {
        run(&plan, env, &ExportControl::default(), |_| {}, |_| {}).unwrap()
    };

    let name = clip.file_name().unwrap().to_string_lossy();
    let name: String = name.chars().take(24).collect();
    for estimate in estimates {
        let (file_type, estimated) = match estimate {
            SizeEstimate::Done { file_type, bytes } => (file_type, bytes),
            other => {
                println!("{name} | {other:?}");
                continue;
            }
        };
        let exported = outputs
            .iter()
            .find_map(|output| match output {
                ExportOutput::Done {
                    file_type: t, size, ..
                } if *t == file_type => Some(*size),
                _ => None,
            })
            .unwrap_or(0);
        let error = (estimated as f64 - exported as f64) / exported as f64 * 100.0;
        println!(
            "{name} | {:.1}-{:.1} s | {}/{} {} | {} | {} | {} | {error:+.1}% | {:.2}",
            case.start,
            case.end,
            case.width,
            case.fps,
            case.direction,
            label(file_type),
            estimated,
            exported,
            wall(file_type)
        );
    }
}

fn label(file_type: FileType) -> &'static str {
    match file_type {
        FileType::Gif => "GIF",
        FileType::Webp => "WebP",
        FileType::Mp4 => "MP4",
    }
}

//! Whole exports with the real ffmpeg on PATH, on tiny clips it generates at test time, checked
//! with ffprobe and by decoding frames: what each file type comes out as, and what a cancelled,
//! failed or colliding run leaves in the destination folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;
use crate::export::domain::args::single_pass_args;
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
        .expect("ffmpeg must be on PATH for the export tests")
}

fn ffprobe() -> PathBuf {
    SearchPath::from_env()
        .locate_tools()
        .ffprobe
        .expect("ffprobe must be on PATH for the export tests")
}

/// Runs `ffmpeg -v error -y <args> <dir/name>` and returns the output path.
fn generate(dir: &Path, name: &str, args: &[&str]) -> PathBuf {
    let out = dir.join(name);
    let output = process::command(&ffmpeg())
        .args(["-v", "error", "-y"])
        .args(args)
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ffmpeg failed making {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    out
}

/// A generated source and the source info its probe would give.
struct Source {
    path: PathBuf,
    info: Value,
}

fn info(width: u32, height: u32, duration: f64, frame_rate: f64, audio: Option<&str>) -> Value {
    json!({
        "duration": duration, "width": width, "height": height, "frameRate": frame_rate,
        "videoCodec": "h264", "container": "mov,mp4,m4a,3gp,3g2,mj2", "audioCodec": audio
    })
}

/// 160×120 at 10 fps for 2 s, with a tone: the general-purpose clip.
fn clip(dir: &Path) -> Source {
    let path = generate(
        dir,
        "clip.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x120:rate=10:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ],
    );
    Source {
        path,
        info: info(160, 120, 2.0, 10.0, Some("aac")),
    }
}

/// Grey frames whose brightness counts the frame: frame n has luma 40 + 8n, at 10 fps for 2 s,
/// lossless, so a decoded output frame says which source frame it came from.
fn counting_clip(dir: &Path) -> Source {
    let path = generate(
        dir,
        "counting.mkv",
        &[
            "-f",
            "lavfi",
            "-i",
            "nullsrc=size=64x48:rate=10:duration=2,geq=lum=40+N*8:cb=128:cr=128",
            "-c:v",
            "ffv1",
        ],
    );
    Source {
        path,
        info: info(64, 48, 2.0, 10.0, None),
    }
}

/// A black 160×120 frame with a red box in its top-left quarter (40×30), for orientation and
/// crop checks, at 10 fps for 1 s.
fn marked_clip(dir: &Path) -> Source {
    let path = generate(
        dir,
        "marked.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=black:size=160x120:rate=10:duration=1,drawbox=x=0:y=0:w=40:h=30:color=red:t=fill",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    Source {
        path,
        info: info(160, 120, 1.0, 10.0, None),
    }
}

/// The request Convert would send for `source`: width 80, 10 fps, every file type, plus `patch`
/// (merged into the request; `settings` merges field by field).
fn request(source: &Source, patch: Value) -> ExportRequest {
    let mut value = json!({
        "path": source.path,
        "info": source.info,
        "settings": {
            "width": 80, "fps": 10, "speed": 1, "direction": "forward", "rotation": 0,
            "flipHorizontal": false, "flipVertical": false, "topText": "", "bottomText": "",
            "captionSize": 12, "fileTypes": ["gif", "webp", "mp4"], "gifColors": 256,
            "gifDither": "sierra2_4a", "gifEncoder": "ffmpeg", "gifskiQuality": 90, "gifskiMotionQuality": 90, "plays": 0, "webpQuality": 80, "keepSound": false
        },
        "keepSound": false
    });
    merge(&mut value, patch);
    serde_json::from_value(value).unwrap()
}

fn merge(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch,
    }
}

fn file_type_of(output: &ExportOutput) -> FileType {
    match output {
        ExportOutput::Done { file_type, .. }
        | ExportOutput::Failed { file_type, .. }
        | ExportOutput::Cancelled { file_type } => *file_type,
    }
}

/// An export's folders and what it reported.
struct Run {
    destination: TempDir,
    work: TempDir,
    outputs: Vec<ExportOutput>,
    progress: Vec<ExportProgress>,
    /// The outputs as `export-output` would have emitted them.
    emitted: Vec<ExportOutput>,
}

impl Run {
    /// The finished file of `file_type`; panics with the reported output otherwise.
    fn file(&self, file_type: FileType) -> PathBuf {
        match self.outputs.iter().find(|o| file_type_of(o) == file_type) {
            Some(ExportOutput::Done { path, .. }) => PathBuf::from(path),
            other => panic!("no {file_type:?} output: {other:?}"),
        }
    }

    fn files(&self) -> Vec<PathBuf> {
        self.outputs
            .iter()
            .filter_map(|o| match o {
                ExportOutput::Done { path, .. } => Some(PathBuf::from(path)),
                _ => None,
            })
            .collect()
    }
}

fn export(plan_for: impl FnOnce(&Path) -> ExportPlan) -> Run {
    let destination = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let plan = plan_for(destination.path());
    let mut progress = Vec::new();
    let mut emitted = Vec::new();
    let outputs = run(
        &plan,
        Environment {
            ffmpeg: &ffmpeg(),
            gifski: gifski(),
            work_dir: work.path(),
            available_memory: None,
        },
        &ExportControl::default(),
        |p| progress.push(p),
        |o| emitted.push(o.clone()),
    )
    .unwrap();
    Run {
        destination,
        work,
        outputs,
        progress,
        emitted,
    }
}

fn export_request(source: &Source, patch: Value) -> Run {
    export(|destination| {
        request(source, patch)
            .validate(destination.to_owned())
            .unwrap()
    })
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// What ffprobe says about an output.
#[derive(Debug)]
struct Probed {
    width: u64,
    height: u64,
    /// The last frame's time plus its duration: the time the animation takes to play once.
    duration: f64,
    audio: bool,
}

fn probe(file: &Path) -> Probed {
    let output = process::command(&ffprobe())
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(file)
        .output()
        .unwrap();
    assert!(output.status.success(), "ffprobe failed on {file:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let streams = json["streams"].as_array().unwrap();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .expect("a video stream");
    let audio = streams.iter().any(|s| s["codec_type"] == "audio");

    // ffprobe gives an animated WebP no duration, so time the frames of every type alike.
    let frames = process::command(&ffprobe())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=pts_time,duration_time",
            "-of",
            "csv=p=0",
        ])
        .arg(file)
        .output()
        .unwrap();
    let text = String::from_utf8(frames.stdout).unwrap();
    let last = text.lines().rfind(|l| !l.trim().is_empty()).unwrap();
    let (pts, duration) = last.trim().trim_end_matches(',').split_once(',').unwrap();
    Probed {
        width: video["width"].as_u64().unwrap(),
        height: video["height"].as_u64().unwrap(),
        duration: pts.parse::<f64>().unwrap() + duration.parse::<f64>().unwrap(),
        audio,
    }
}

fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual}, expected {expected} ± {tolerance}"
    );
}

/// The frame of `file` shown at `seconds`, a frame's start time, decoded as `pix_fmt` raw bytes.
/// By time rather than index because libwebp_anim merges identical neighbouring frames.
fn frame_at(file: &Path, seconds: f64, pix_fmt: &str) -> Vec<u8> {
    let output = process::command(&ffmpeg())
        .args(["-v", "error", "-i"])
        .arg(file)
        .args([
            "-vf",
            // The first frame starting at or after `seconds`, less a millisecond of rounding.
            &format!("select=gte(t\\,{})", seconds - 0.001),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            pix_fmt,
            "-",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success() && !output.stdout.is_empty(),
        "no frame at {seconds} s in {file:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// Mean brightness of the frame at `seconds`, decoded to RGB: which frame of [`counting_clip`] it
/// shows. RGB rather than luma because every file type decodes to it alike, GIF having no YUV.
fn brightness(file: &Path, seconds: f64) -> f64 {
    let rgb = frame_at(file, seconds, "rgb24");
    rgb.iter().map(|&v| f64::from(v)).sum::<f64>() / rgb.len() as f64
}

/// The brightness [`counting_clip`]'s frame `n` decodes to: its luma, 40 + 8n, expanded from
/// video range (16 to 235) to RGB's 0 to 255. Neighbouring frames differ by about 9.
fn counted(n: u32) -> f64 {
    (f64::from(40 + 8 * n) - 16.0) * 255.0 / 219.0
}

/// Whether the pixel at (`x`, `y`) of the first frame is red rather than black.
fn is_red(file: &Path, x: usize, y: usize) -> bool {
    let probed = probe(file);
    let rgb = frame_at(file, 0.0, "rgb24");
    let at = (y * probed.width as usize + x) * 3;
    let (r, g, b) = (rgb[at], rgb[at + 1], rgb[at + 2]);
    r > 150 && g < 90 && b < 90
}

/// The GIF's loop extension count (0 forever, n repeats), or `None` without one (plays once).
fn gif_loop(file: &Path) -> Option<u16> {
    let bytes = fs::read(file).unwrap();
    let at = bytes.windows(11).position(|w| w == b"NETSCAPE2.0")?;
    // Sub-block: size 3, id 1, then the count, little-endian.
    let block = &bytes[at + 11..at + 15];
    assert_eq!(block[..2], [3, 1]);
    Some(u16::from_le_bytes([block[2], block[3]]))
}

/// The WebP's ANIM loop count (0 forever).
fn webp_loop(file: &Path) -> u16 {
    let bytes = fs::read(file).unwrap();
    let at = bytes
        .windows(4)
        .position(|w| w == b"ANIM")
        .expect("ANIM chunk");
    // Chunk: fourcc, size, background colour, then the count, little-endian.
    u16::from_le_bytes([bytes[at + 12], bytes[at + 13]])
}

use FileType::{Gif, Mp4, Webp};

#[test]
fn one_run_makes_every_ticked_file_type_in_order() {
    let dir = TempDir::new().unwrap();
    let run = export_request(&clip(dir.path()), json!({}));

    assert_eq!(
        names(run.destination.path()),
        ["clip.gif", "clip.mp4", "clip.webp"]
    );
    assert_eq!(
        run.outputs.iter().map(file_type_of).collect::<Vec<_>>(),
        [Gif, Webp, Mp4]
    );
    assert_eq!(run.emitted, run.outputs);
    for file_type in [Gif, Webp, Mp4] {
        let probed = probe(&run.file(file_type));
        assert_eq!((probed.width, probed.height), (80, 60), "{file_type:?}");
        assert_close(
            probed.duration,
            2.0,
            0.11,
            &format!("{file_type:?} duration"),
        );
        assert!(!probed.audio, "{file_type:?} has no sound unless asked");
        match &run.outputs[file_type as usize] {
            ExportOutput::Done {
                path,
                size,
                width,
                height,
                ..
            } => {
                assert_eq!(fs::metadata(path).unwrap().len(), *size);
                // The reported size is the encoded one, which the UI shows beside the output.
                assert_eq!(
                    (u64::from(*width), u64::from(*height)),
                    (probed.width, probed.height)
                );
            }
            other => panic!("{other:?}"),
        }
        // Each output's progress runs from 0 to 1, in run order.
        let progress: Vec<f64> = run
            .progress
            .iter()
            .filter(|p| p.file_type == file_type)
            .map(|p| p.progress)
            .collect();
        assert_eq!(progress.first(), Some(&0.0), "{file_type:?}");
        assert_eq!(progress.last(), Some(&1.0), "{file_type:?}");
    }
    let order: Vec<FileType> = run.progress.iter().map(|p| p.file_type).collect();
    assert!(order.is_sorted(), "{order:?}");
    // Forever, by default.
    assert_eq!(gif_loop(&run.file(Gif)), Some(0));
    assert_eq!(webp_loop(&run.file(Webp)), 0);
}

#[test]
fn plays_set_the_gif_and_webp_loop_counts() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let types = json!(["gif", "webp"]);
    let once = export_request(
        &source,
        json!({ "settings": { "plays": 1, "fileTypes": types } }),
    );
    assert_eq!(gif_loop(&once.file(Gif)), None);
    assert_eq!(webp_loop(&once.file(Webp)), 1);
    let thrice = export_request(
        &source,
        json!({ "settings": { "plays": 3, "fileTypes": types } }),
    );
    assert_eq!(gif_loop(&thrice.file(Gif)), Some(2));
    assert_eq!(webp_loop(&thrice.file(Webp)), 3);
}

/// Makes `file_type` from `plan` with mgck's command alone, without the app's progress reporting,
/// into `dir`.
fn reference_output(plan: &ExportPlan, file_type: FileType, dir: &Path) -> PathBuf {
    let out = dir.join(format!("reference.{}", file_type.extension()));
    let args = single_pass_args(&SinglePass::from_plan(plan, file_type, &out, None));
    let output = process::command(&ffmpeg())
        .args(["-v", "error", "-y"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    out
}

#[test]
fn gif_and_webp_progress_moves_through_the_run_and_leaves_the_files_unchanged() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    for direction in ["forward", "reverse"] {
        let patch = json!({ "settings": { "fileTypes": ["gif", "webp"], "direction": direction } });
        let plan = request(&source, patch.clone())
            .validate(dir.path().to_owned())
            .unwrap();
        let run = export_request(&source, patch);
        for file_type in [Gif, Webp] {
            let what = format!("{direction} {file_type:?}");
            let progress: Vec<f64> = run
                .progress
                .iter()
                .filter(|p| p.file_type == file_type)
                .map(|p| p.progress)
                .collect();
            // ffmpeg's own report sat at 0 until a GIF's palette or the reversed frames were
            // ready, and a WebP's until its one packet was written at the end.
            let between = progress.iter().filter(|&&p| p > 0.0 && p < 1.0).count();
            assert!(between >= 3, "{what}: {progress:?}");
            assert_eq!(progress.first(), Some(&0.0), "{what}");
            assert_eq!(progress.last(), Some(&1.0), "{what}");
            assert!(progress.is_sorted(), "{what}: {progress:?}");
            if file_type == Gif || direction == "reverse" {
                // The tap reports while the input is read, before anything reaches the encoder.
                assert!(
                    progress.iter().any(|&p| p > 0.0 && p < 0.2),
                    "{what}: {progress:?}"
                );
            }

            let file = run.file(file_type);
            let reference = reference_output(&plan, file_type, dir.path());
            assert!(
                fs::read(&file).unwrap() == fs::read(&reference).unwrap(),
                "{what} differs from mgck's command's output"
            );
            let probed = probe(&file);
            assert_eq!((probed.width, probed.height), (80, 60), "{what}");
            assert_close(probed.duration, 2.0, 0.11, &format!("{what} duration"));
        }
        assert_eq!(gif_loop(&run.file(Gif)), Some(0));
        assert_eq!(webp_loop(&run.file(Webp)), 0);
    }
}

#[test]
fn a_trim_keeps_only_the_selection() {
    let dir = TempDir::new().unwrap();
    let run = export_request(
        &clip(dir.path()),
        json!({ "trim": { "start": 0.5, "end": 1.5 }, "keepSound": true }),
    );
    for file_type in [Gif, Webp, Mp4] {
        let probed = probe(&run.file(file_type));
        assert_close(
            probed.duration,
            1.0,
            0.11,
            &format!("{file_type:?} duration"),
        );
    }
}

#[test]
fn a_trim_starts_on_the_frame_the_player_showed() {
    let dir = TempDir::new().unwrap();
    let source = counting_clip(dir.path());
    let types = json!({ "fileTypes": ["mp4"], "width": "original" });
    // Between frames 3 (0.3 s) and 4: the player shows frame 3. On frame 3's own start, too.
    for start in [0.35, 0.3] {
        let run = export_request(
            &source,
            json!({ "trim": { "start": start, "end": start + 1.0 }, "settings": types }),
        );
        let file = run.file(Mp4);
        assert_close(
            brightness(&file, 0.0),
            counted(3),
            4.0,
            &format!("first frame at {start}"),
        );
        assert_close(probe(&file).duration, 1.0, 0.11, "duration");
    }
}

#[test]
fn a_crop_keeps_its_box_of_the_frame() {
    let dir = TempDir::new().unwrap();
    let source = marked_clip(dir.path());
    // The red box is the top-left quarter: 40×30 pixels.
    let run = export_request(
        &source,
        json!({
            "crop": { "x": 0.0, "y": 0.0, "width": 0.25, "height": 0.25 },
            "settings": { "width": "original", "fileTypes": ["gif", "mp4"] }
        }),
    );
    for file_type in [Gif, Mp4] {
        let file = run.file(file_type);
        let probed = probe(&file);
        assert_eq!((probed.width, probed.height), (40, 30), "{file_type:?}");
        assert!(is_red(&file, 20, 15), "{file_type:?} keeps the red box");
    }
    let elsewhere = export_request(
        &source,
        json!({
            "crop": { "x": 0.5, "y": 0.5, "width": 0.5, "height": 0.5 },
            "settings": { "width": "original", "fileTypes": ["mp4"] }
        }),
    );
    assert!(!is_red(&elsewhere.file(Mp4), 40, 30));
}

#[test]
fn a_rotated_source_is_cropped_and_sized_as_displayed() {
    let dir = TempDir::new().unwrap();
    let marked = marked_clip(dir.path());
    let marked_path = marked.path.to_str().unwrap();
    let path = generate(
        dir.path(),
        "rotated.mp4",
        &["-display_rotation", "90", "-i", marked_path, "-c", "copy"],
    );
    // Stored 160×120 with a quarter-turn display matrix: shown, probed and exported 120×160.
    let rotated = Source {
        path,
        info: info(120, 160, 1.0, 10.0, None),
    };
    let run = export_request(
        &rotated,
        json!({
            "crop": { "x": 0.0, "y": 0.0, "width": 1.0, "height": 0.5 },
            "settings": { "width": "original" }
        }),
    );
    for file_type in [Gif, Webp, Mp4] {
        let probed = probe(&run.file(file_type));
        assert_eq!((probed.width, probed.height), (120, 80), "{file_type:?}");
    }
}

#[test]
fn rotate_and_flip_turn_and_mirror_the_output() {
    let dir = TempDir::new().unwrap();
    let source = marked_clip(dir.path());
    let settings = |rotation: u16, h: bool, v: bool, types: Value| {
        json!({ "settings": {
            "width": "original", "rotation": rotation, "flipHorizontal": h,
            "flipVertical": v, "fileTypes": types
        }})
    };
    // A clockwise quarter turn takes the top-left box to the top-right of a 120×160 frame.
    let turned = export_request(&source, settings(90, false, false, json!(["mp4"])));
    let file = turned.file(Mp4);
    let probed = probe(&file);
    assert_eq!((probed.width, probed.height), (120, 160));
    assert!(is_red(&file, 110, 10) && !is_red(&file, 10, 10));
    // Then mirroring left to right brings it back to the top-left, on every file type.
    let mirrored = export_request(
        &source,
        settings(90, true, false, json!(["gif", "webp", "mp4"])),
    );
    for file_type in [Gif, Webp, Mp4] {
        let file = mirrored.file(file_type);
        assert!(is_red(&file, 10, 10), "{file_type:?}");
        assert!(!is_red(&file, 110, 10), "{file_type:?}");
    }
    // And top to bottom as well, to the bottom-left.
    let both = export_request(&source, settings(90, true, true, json!(["mp4"])));
    let file = both.file(Mp4);
    assert!(is_red(&file, 10, 150) && !is_red(&file, 10, 10));
}

#[test]
fn speed_changes_the_length_and_mp4_sound_keeps_up() {
    let dir = TempDir::new().unwrap();
    let run = export_request(
        &clip(dir.path()),
        json!({ "keepSound": true, "settings": { "speed": 2 } }),
    );
    for file_type in [Gif, Webp, Mp4] {
        let probed = probe(&run.file(file_type));
        assert_close(
            probed.duration,
            1.0,
            0.11,
            &format!("{file_type:?} duration"),
        );
        assert_eq!(probed.audio, file_type == Mp4, "{file_type:?} sound");
    }
}

#[test]
fn sound_is_kept_only_in_mp4_and_only_when_asked() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let mp4 = json!({ "fileTypes": ["mp4"] });
    let kept = export_request(&source, json!({ "keepSound": true, "settings": mp4 }));
    assert!(probe(&kept.file(Mp4)).audio);
    let dropped = export_request(&source, json!({ "keepSound": false, "settings": mp4 }));
    assert!(!probe(&dropped.file(Mp4)).audio);
    // Nothing plays sound backward, even when asked.
    let reversed = export_request(
        &source,
        json!({ "keepSound": true, "settings": { "fileTypes": ["mp4"], "direction": "reverse" } }),
    );
    assert!(!probe(&reversed.file(Mp4)).audio);
}

#[test]
fn reverse_plays_the_selection_backward() {
    let dir = TempDir::new().unwrap();
    let run = export_request(
        &counting_clip(dir.path()),
        json!({ "settings": { "direction": "reverse", "width": "original" } }),
    );
    for file_type in [Gif, Webp, Mp4] {
        let file = run.file(file_type);
        assert_close(
            probe(&file).duration,
            2.0,
            0.11,
            &format!("{file_type:?} duration"),
        );
        assert_close(
            brightness(&file, 0.0),
            counted(19),
            4.0,
            &format!("{file_type:?} first frame"),
        );
        // Midway rather than the last frame: the GIF's diff-based palette (`stats_mode=diff`)
        // can miss a colour only the final frame brings, which says nothing about direction.
        assert_close(
            brightness(&file, 1.0),
            counted(9),
            4.0,
            &format!("{file_type:?} middle frame"),
        );
    }
}

#[test]
fn boomerang_plays_forward_then_backward_for_twice_as_long() {
    let dir = TempDir::new().unwrap();
    let run = export_request(
        &counting_clip(dir.path()),
        json!({ "settings": { "direction": "boomerang", "width": "original" } }),
    );
    for file_type in [Gif, Webp, Mp4] {
        let file = run.file(file_type);
        let what = |frame: &str| format!("{file_type:?} {frame}");
        assert_close(probe(&file).duration, 4.0, 0.11, &what("duration"));
        assert_close(
            brightness(&file, 0.0),
            counted(0),
            4.0,
            &what("first frame"),
        );
        assert_close(
            brightness(&file, 1.9),
            counted(19),
            4.0,
            &what("turning frame"),
        );
        assert_close(brightness(&file, 3.9), counted(0), 4.0, &what("last frame"));
    }
}

/// A transparent `width`×`height` PNG with an opaque red band across its top 10 rows: captions as
/// `renderCaptionImage` would draw them.
fn caption_png(dir: &Path, width: u32, height: u32) -> Vec<u8> {
    let path = generate(
        dir,
        "captions.png",
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("color=c=red:size={width}x10"),
            "-vf",
            &format!("format=rgba,pad={width}:{height}:0:0:color=black@0"),
            "-frames:v",
            "1",
        ],
    );
    fs::read(path).unwrap()
}

#[test]
fn captions_are_overlaid_on_every_file_type_and_their_image_deleted() {
    let dir = TempDir::new().unwrap();
    let source = marked_clip(dir.path());
    let png = caption_png(dir.path(), 80, 60);
    let run = export_request(&source, json!({ "captions": png }));
    for file_type in [Gif, Webp, Mp4] {
        let file = run.file(file_type);
        assert!(is_red(&file, 60, 4), "{file_type:?} shows the caption band");
        assert!(!is_red(&file, 60, 40), "{file_type:?} is clear below it");
    }
    assert!(
        names(run.work.path()).is_empty(),
        "caption image left behind"
    );
}

#[test]
fn a_cancelled_run_leaves_nothing_in_the_destination() {
    let dir = TempDir::new().unwrap();
    // Long enough that the MP4 is still encoding when the cancel lands.
    let path = generate(
        dir.path(),
        "long.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=30:duration=20",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    let source = Source {
        path,
        info: info(320, 240, 20.0, 30.0, None),
    };
    let png = caption_png(dir.path(), 320, 240);
    let destination = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let plan = request(
        &source,
        json!({
            "captions": png,
            "settings": { "width": "original", "fps": 30, "fileTypes": ["webp", "mp4"] }
        }),
    )
    .validate(destination.path().to_owned())
    .unwrap();
    let control = ExportControl::default();
    let mut progress = Vec::new();

    let (outputs, seen) = thread::scope(|scope| {
        // Cancel once ffmpeg has created its file in the folder, mid-encode.
        let watcher = scope.spawn(|| {
            loop {
                if let Some(name) = names(destination.path()).into_iter().next() {
                    control.cancel(false);
                    return name;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        let env = Environment {
            ffmpeg: &ffmpeg(),
            gifski: gifski(),
            work_dir: work.path(),
            available_memory: None,
        };
        let outputs = run(&plan, env, &control, |p| progress.push(p), |_| {}).unwrap();
        (outputs, watcher.join().unwrap())
    });

    assert!(
        seen.contains(".part-"),
        "cancelled before ffmpeg finished: {seen}"
    );
    assert_eq!(
        outputs,
        [
            ExportOutput::Cancelled { file_type: Webp },
            ExportOutput::Cancelled { file_type: Mp4 },
        ]
    );
    assert!(
        names(destination.path()).is_empty(),
        "partial file left behind"
    );
    assert!(names(work.path()).is_empty(), "caption image left behind");
    assert!(progress.iter().all(|p| p.progress < 1.0));
}

#[test]
fn cancelling_mid_run_keeps_the_finished_outputs_and_leaves_no_partial_file() {
    let dir = TempDir::new().unwrap();
    // Long enough that the WebP is still encoding when the cancel lands.
    let path = generate(
        dir.path(),
        "long.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=30:duration=20",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    let source = Source {
        path,
        info: info(320, 240, 20.0, 30.0, None),
    };
    let destination = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let plan = request(
        &source,
        json!({ "settings": { "width": "original", "fps": 30, "fileTypes": ["gif", "webp", "mp4"] } }),
    )
    .validate(destination.path().to_owned())
    .unwrap();
    let control = ExportControl::default();
    let mut emitted = Vec::new();

    let (outputs, seen) = thread::scope(|scope| {
        // Cancel once the WebP's partial file appears, mid-encode, with the GIF already saved.
        let watcher = scope.spawn(|| {
            loop {
                if let Some(name) = names(destination.path())
                    .into_iter()
                    .find(|name| name.contains(".part-") && name.ends_with(".webp"))
                {
                    control.cancel(false);
                    return name;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        let env = Environment {
            ffmpeg: &ffmpeg(),
            gifski: gifski(),
            work_dir: work.path(),
            available_memory: None,
        };
        let outputs = run(&plan, env, &control, |_| {}, |o| emitted.push(o.clone())).unwrap();
        (outputs, watcher.join().unwrap())
    });

    assert!(seen.starts_with("long.part-"), "{seen}");
    let gif = destination.path().join("long.gif");
    assert!(matches!(
        &outputs[0],
        ExportOutput::Done { file_type: Gif, path, .. } if Path::new(path) == gif
    ));
    assert_eq!(
        outputs[1..],
        [
            ExportOutput::Cancelled { file_type: Webp },
            ExportOutput::Cancelled { file_type: Mp4 },
        ]
    );
    assert_eq!(emitted, outputs, "each output is emitted as it settles");
    assert_eq!(
        names(destination.path()),
        ["long.gif"],
        "only the finished output is left"
    );
    assert_eq!(probe(&gif).width, 320);
}

#[test]
fn a_failed_file_type_leaves_nothing_and_the_rest_still_run() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let run = export(|destination| {
        let mut plan = request(
            &source,
            json!({ "settings": { "fileTypes": ["gif", "mp4"] } }),
        )
        .validate(destination.to_owned())
        .unwrap();
        // An invalid setting slipped past validation: palettegen takes 2 to 256 colours.
        plan.selection.settings.gif_colors = 1;
        plan
    });
    match &run.outputs[0] {
        ExportOutput::Failed { file_type, reason } => {
            assert_eq!(*file_type, Gif);
            assert!(
                reason.contains("max_colors"),
                "ffmpeg's error tail: {reason}"
            );
        }
        other => panic!("expected the GIF to fail, got {other:?}"),
    }
    assert!(matches!(
        run.outputs[1],
        ExportOutput::Done { file_type: Mp4, .. }
    ));
    assert_eq!(names(run.destination.path()), ["clip.mp4"]);
}

#[test]
fn an_existing_name_gets_a_number_and_is_not_overwritten() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let run = export(|destination| {
        fs::write(destination.join("clip.gif"), b"mine").unwrap();
        request(&source, json!({ "settings": { "fileTypes": ["gif"] } }))
            .validate(destination.to_owned())
            .unwrap()
    });
    assert_eq!(run.files(), [run.destination.path().join("clip (1).gif")]);
    assert_eq!(
        fs::read(run.destination.path().join("clip.gif")).unwrap(),
        b"mine"
    );
    assert_eq!(probe(&run.file(Gif)).width, 80);
}

#[test]
fn a_run_needing_more_memory_than_is_free_is_refused_before_ffmpeg_starts() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let destination = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let png = caption_png(dir.path(), 80, 60);
    let plan = request(
        &source,
        json!({ "captions": png, "settings": { "direction": "reverse" } }),
    )
    .validate(destination.path().to_owned())
    .unwrap();
    // 2 s of 80×60 at 10 fps, two copies: 20 × 80 × 60 × 4 × 2 bytes.
    let held = 20 * 80 * 60 * 4 * 2;
    let env = |available| Environment {
        ffmpeg: Path::new("no-ffmpeg-needed"),
        gifski: None,
        work_dir: work.path(),
        available_memory: Some(available),
    };
    let result = run(
        &plan,
        env(held - 1),
        &ExportControl::default(),
        |_| {},
        |_| {},
    );
    assert_eq!(
        result,
        Err(ExportError::NotEnoughMemory {
            held_bytes: held,
            available_bytes: held - 1
        })
    );
    assert!(names(destination.path()).is_empty());
    assert!(names(work.path()).is_empty());
}

#[test]
fn a_run_whose_paging_was_accepted_goes_ahead() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let destination = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let plan = request(
        &source,
        json!({ "acceptPaging": true, "settings": { "direction": "reverse", "fileTypes": ["gif"] } }),
    )
    .validate(destination.path().to_owned())
    .unwrap();
    let outputs = run(
        &plan,
        Environment {
            ffmpeg: &ffmpeg(),
            gifski: gifski(),
            work_dir: work.path(),
            available_memory: Some(1),
        },
        &ExportControl::default(),
        |_| {},
        |_| {},
    )
    .unwrap();
    assert!(
        matches!(&outputs[..], [ExportOutput::Done { .. }]),
        "{outputs:?}"
    );
}

#[test]
fn one_export_runs_at_a_time() {
    let exports = Exports::default();
    let running = exports.begin().unwrap();
    assert!(matches!(exports.begin(), Err(ExportError::Busy)));
    exports.cancel();
    assert!(running.control().is_cancelled());
    drop(running);
    assert!(exports.begin().is_ok());
}

// gifski GIFs: gifski must be on PATH for these, as ffmpeg is for the rest.

/// Frames ffprobe decodes from `file`.
fn frame_count(file: &Path) -> u64 {
    let output = process::command(&ffprobe())
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn gifski_patch(settings: Value) -> Value {
    let mut patch = json!({ "settings": { "gifEncoder": "gifski", "fileTypes": ["gif"] } });
    merge(&mut patch, json!({ "settings": settings }));
    patch
}

#[test]
fn a_gifski_gif_has_the_selections_frames_size_and_plays() {
    assert!(gifski().is_some(), "gifski must be on PATH for this test");
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let run = export_request(&source, gifski_patch(json!({})));
    let gif = run.file(Gif);
    let probed = probe(&gif);
    assert_eq!((probed.width, probed.height), (80, 60));
    // 2 s at 10 fps.
    assert_eq!(frame_count(&gif), 20);
    assert_close(probed.duration, 2.0, 0.11, "gifski GIF duration");
    assert_eq!(gif_loop(&gif), Some(0), "forever by default");
    let progress: Vec<f64> = run.progress.iter().map(|p| p.progress).collect();
    assert_eq!(progress.first(), Some(&0.0));
    assert_eq!(progress.last(), Some(&1.0));
    assert!(progress.is_sorted(), "{progress:?}");
    assert!(
        progress.iter().filter(|&&p| p > 0.0 && p < 1.0).count() >= 3,
        "{progress:?}"
    );

    // A trimmed, sped-up selection at another frame rate and width: 1 s at 2x plays 0.5 s, 4
    // frames at 8 fps.
    let trimmed = |direction: &str| {
        let mut patch =
            gifski_patch(json!({ "width": 120, "fps": 8, "speed": 2, "direction": direction }));
        merge(&mut patch, json!({ "trim": { "start": 0.5, "end": 1.5 } }));
        // The run owns the folder the GIF is in.
        export_request(&source, patch)
    };
    let forward = trimmed("forward");
    let gif = forward.file(Gif);
    let probed = probe(&gif);
    assert_eq!((probed.width, probed.height), (120, 90));
    assert_eq!(frame_count(&gif), 4);
    assert_close(probed.duration, 0.5, 0.06, "trimmed gifski GIF duration");
    // Boomerang plays it there and back. gifski stores the turn's repeated frame once, held for
    // two frames' time, so the GIF has one frame fewer but plays as long.
    let boomerang = trimmed("boomerang");
    let gif = boomerang.file(Gif);
    assert_eq!(frame_count(&gif), 7);
    assert_close(
        probe(&gif).duration,
        1.0,
        0.06,
        "boomerang gifski GIF duration",
    );

    for (plays, expected) in [(1, None), (3, Some(2))] {
        let run = export_request(&source, gifski_patch(json!({ "plays": plays })));
        assert_eq!(gif_loop(&run.file(Gif)), expected, "plays {plays}");
    }
}

#[test]
fn a_gifski_gif_keeps_the_sources_colours() {
    let dir = TempDir::new().unwrap();
    let source = marked_clip(dir.path());
    let run = export_request(&source, gifski_patch(json!({ "width": 160 })));
    let gif = run.file(Gif);
    assert!(is_red(&gif, 10, 10), "the red box stays red");
    assert!(!is_red(&gif, 100, 100), "the black stays black");
}

/// Whether a process with `pid` is still running.
#[cfg(windows)]
fn alive(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
}

#[cfg(windows)]
#[test]
fn cancelling_a_gifski_gif_kills_ffmpeg_and_gifski_and_leaves_no_file() {
    let dir = TempDir::new().unwrap();
    let path = generate(
        dir.path(),
        "long.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=30:duration=20",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    let source = Source {
        path,
        info: info(320, 240, 20.0, 30.0, None),
    };
    let destination = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let plan = request(
        &source,
        gifski_patch(json!({ "width": "original", "fps": 30 })),
    )
    .validate(destination.path().to_owned())
    .unwrap();
    let control = ExportControl::default();
    let mut progress = Vec::new();

    let (outputs, pids) = thread::scope(|scope| {
        // Cancel once gifski has written part of the GIF and ffmpeg has reported frames.
        let watcher = scope.spawn(|| {
            loop {
                let job = lock(&control.state).job.clone();
                let part = names(destination.path())
                    .into_iter()
                    .find(|name| name.contains(".part-"));
                if let (Some(job), Some(part)) = (job, part)
                    && fs::metadata(destination.path().join(part)).is_ok_and(|m| m.len() > 0)
                {
                    let pids = job.pids();
                    control.cancel(false);
                    return pids;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        let env = Environment {
            ffmpeg: &ffmpeg(),
            gifski: gifski(),
            work_dir: work.path(),
            available_memory: None,
        };
        let outputs = run(&plan, env, &control, |p| progress.push(p), |_| {}).unwrap();
        (outputs, watcher.join().unwrap())
    });

    assert_eq!(outputs, [ExportOutput::Cancelled { file_type: Gif }]);
    assert_eq!(pids.len(), 2, "ffmpeg and gifski");
    for pid in pids {
        assert!(!alive(pid), "process {pid} outlived the cancel");
    }
    assert!(
        names(destination.path()).is_empty(),
        "partial file left behind"
    );
    assert!(progress.iter().any(|p| p.progress > 0.0), "{progress:?}");
    assert!(progress.iter().all(|p| p.progress < 1.0));
}

#[test]
fn a_failing_gifski_is_named_with_its_own_error() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let run = export(|destination| {
        let mut plan = request(
            &source,
            gifski_patch(json!({ "fileTypes": ["gif", "mp4"] })),
        )
        .validate(destination.to_owned())
        .unwrap();
        // An invalid setting slipped past validation: gifski takes qualities 1 to 100.
        plan.selection.settings.gifski_quality = 0;
        plan
    });
    match &run.outputs[0] {
        ExportOutput::Failed { file_type, reason } => {
            assert_eq!(*file_type, Gif);
            assert!(reason.starts_with("gifski: "), "{reason}");
            assert!(reason.contains("quality"), "{reason}");
        }
        other => panic!("expected the GIF to fail, got {other:?}"),
    }
    assert!(matches!(
        run.outputs[1],
        ExportOutput::Done { file_type: Mp4, .. }
    ));
    assert_eq!(names(run.destination.path()), ["clip.mp4"]);
}

#[test]
fn a_failing_ffmpeg_in_front_of_gifski_is_reported_as_ffmpegs_error() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let run = export(|destination| {
        let mut plan = request(&source, gifski_patch(json!({})))
            .validate(destination.to_owned())
            .unwrap();
        // A source that turned into something else since its probe: ffmpeg cannot open it, and
        // gifski, given no frames, fails after it.
        let broken = dir.path().join("broken.mp4");
        fs::write(&broken, b"not a video").unwrap();
        plan.source = broken;
        plan
    });
    match &run.outputs[0] {
        ExportOutput::Failed { reason, .. } => {
            assert!(!reason.starts_with("gifski"), "{reason}");
            assert!(reason.contains("broken.mp4"), "{reason}");
        }
        other => panic!("expected the GIF to fail, got {other:?}"),
    }
    assert!(names(run.destination.path()).is_empty());
}

#[test]
fn a_gifski_gif_without_gifski_on_path_fails_and_the_rest_still_run() {
    let dir = TempDir::new().unwrap();
    let source = clip(dir.path());
    let destination = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let plan = request(
        &source,
        gifski_patch(json!({ "fileTypes": ["gif", "webp"] })),
    )
    .validate(destination.path().to_owned())
    .unwrap();
    let env = Environment {
        ffmpeg: &ffmpeg(),
        gifski: None,
        work_dir: work.path(),
        available_memory: None,
    };
    let outputs = run(&plan, env, &ExportControl::default(), |_| {}, |_| {}).unwrap();
    assert_eq!(
        outputs[0],
        ExportOutput::Failed {
            file_type: Gif,
            reason: "gifski was not found on PATH".to_owned()
        }
    );
    assert!(matches!(
        outputs[1],
        ExportOutput::Done {
            file_type: Webp,
            ..
        }
    ));
    assert_eq!(names(destination.path()), ["clip.webp"]);
}

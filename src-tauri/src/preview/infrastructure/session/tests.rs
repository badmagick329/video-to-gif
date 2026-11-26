//! The open source's proxy lifecycle, driven through [`Previewer`] with the real ffmpeg on PATH
//! and clips it generates at test time.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::Previewer;
use crate::preview::domain::{PreviewError, PreviewSource, TimeRange};
use crate::probe::domain::SourceInfo;
use crate::process;
use crate::search_path::SearchPath;
use crate::temp_folder::{ROOT_NAME, TempFolder};

fn ffmpeg_path() -> Option<PathBuf> {
    SearchPath::from_env().locate_tools().ffmpeg
}

fn ffprobe_path() -> PathBuf {
    SearchPath::from_env()
        .locate_tools()
        .ffprobe
        .expect("ffprobe must be on PATH for the preview tests")
}

/// Runs `ffmpeg -v error -y <args> <dir/name>` and returns the output path.
fn generate(dir: &Path, name: &str, args: &[&str]) -> PathBuf {
    let out = dir.join(name);
    let output = process::command(&ffmpeg_path().expect("ffmpeg must be on PATH"))
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

/// ffprobe's `-show_format -show_streams` JSON for `file`.
fn ffprobe(file: &Path) -> serde_json::Value {
    let output = process::command(&ffprobe_path())
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(file)
        .output()
        .unwrap();
    assert!(output.status.success(), "ffprobe failed on {file:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

fn seconds(value: &serde_json::Value) -> f64 {
    value.as_str().unwrap().parse().unwrap()
}

fn stream<'a>(probe: &'a serde_json::Value, kind: &str) -> Option<&'a serde_json::Value> {
    probe["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == kind)
}

fn info(width: u32, height: u32, duration: f64, codec: &str, has_audio: bool) -> SourceInfo {
    SourceInfo {
        duration,
        width,
        height,
        frame_rate: 25.0,
        video_codec: codec.to_owned(),
        container: "matroska,webm".to_owned(),
        audio_codec: has_audio.then(|| "aac".to_owned()),
    }
}

/// A previewer over a fresh temp root; the [`TempDir`] keeps the root alive.
fn previewer() -> (TempDir, PathBuf, Previewer) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join(ROOT_NAME);
    let previewer = Previewer::new(TempFolder::create(&root).unwrap());
    (temp, root, previewer)
}

/// Files in the session folder other than its lock.
fn proxies_in(previewer: &Previewer) -> Vec<PathBuf> {
    std::fs::read_dir(previewer.proxy_dir())
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.file_name().unwrap() != "lock")
        .collect()
}

/// An HEVC MKV with audio, the kind WebView2 cannot play, 1440x1080 so the proxy must shrink.
fn hevc_mkv(dir: &Path) -> PathBuf {
    generate(
        dir,
        "hevc.mkv",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1440x1080:rate=25:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-c:v",
            "libx265",
            "-preset",
            "ultrafast",
            "-x265-params",
            "log-level=error",
            "-c:a",
            "aac",
            "-shortest",
        ],
    )
}

/// A 1080p H.264 clip long enough that its proxy is still building when a test interrupts it.
fn long_clip(dir: &Path, name: &str) -> PathBuf {
    generate(
        dir,
        name,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1920x1080:rate=30:duration=20",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ],
    )
}

#[test]
fn hevc_mkv_gets_a_playable_h264_proxy_as_long_as_the_source() {
    let clips = TempDir::new().unwrap();
    let source = hevc_mkv(clips.path());
    let source_probe = ffprobe(&source);
    let (_temp, _root, previewer) = previewer();

    let opened = previewer.open(source.clone(), info(1440, 1080, 2.0, "hevc", true));
    assert_eq!(opened, PreviewSource::Proxy);
    let mut progress = Vec::new();
    let proxy = previewer
        .proxy(&source, ffmpeg_path, |p| progress.push(p))
        .unwrap();

    assert!(proxy.starts_with(previewer.proxy_dir()), "{proxy:?}");
    let proxy_probe = ffprobe(&proxy);
    let video = stream(&proxy_probe, "video").unwrap();
    assert_eq!(video["codec_name"], "h264");
    assert_eq!(video["pix_fmt"], "yuv420p");
    assert_eq!(
        (video["width"].as_u64(), video["height"].as_u64()),
        (Some(960), Some(720))
    );
    assert!(
        proxy_probe["format"]["format_name"]
            .as_str()
            .unwrap()
            .contains("mp4")
    );
    assert_eq!(stream(&proxy_probe, "audio").unwrap()["codec_name"], "aac");

    let frame = 1.0 / 25.0;
    let source_duration = seconds(&source_probe["format"]["duration"]);
    let proxy_duration = seconds(&proxy_probe["format"]["duration"]);
    assert!(
        (proxy_duration - source_duration).abs() <= 2.0 * frame,
        "source {source_duration}s, proxy {proxy_duration}s"
    );
    let proxy_video_duration = seconds(&video["duration"]);
    assert!(
        (proxy_video_duration - 2.0).abs() <= 2.0 * frame,
        "{proxy_video_duration}"
    );

    assert_eq!(progress.last(), Some(&1.0), "{progress:?}");
    assert!(progress.windows(2).all(|w| w[0] <= w[1]), "{progress:?}");
}

/// The full-range mean luma of the first frame at or after `at` seconds into `file`, measured as
/// ffmpeg's input `-ss` (and so export) measures time: from the file's first timestamp.
fn luma_at(file: &Path, at: f64) -> u8 {
    let output = process::command(&ffmpeg_path().unwrap())
        .args(["-v", "error", "-ss", &format!("{at}"), "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-vf", "scale=1:1:flags=area"])
        .args(["-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .unwrap();
    assert!(
        output.status.success() && output.stdout.len() == 1,
        "{output:?}"
    );
    output.stdout[0]
}

#[test]
fn a_time_in_the_proxy_shows_the_same_frame_as_in_the_source() {
    let clips = TempDir::new().unwrap();
    // Frame n's brightness encodes n (limited-range luma 16 + 7n, read back full-range as about
    // 8.15n), and the file starts at 2.5 s rather than 0, as recordings cut from a stream do.
    let source = generate(
        clips.path(),
        "numbered.mkv",
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=black:size=128x128:rate=10:duration=3,format=yuv420p,\
             geq=lum='16+N*7':cb=128:cr=128",
            "-c:v",
            "libx265",
            "-preset",
            "ultrafast",
            "-x265-params",
            "log-level=error",
            "-output_ts_offset",
            "2.5",
        ],
    );
    let source_start = seconds(&ffprobe(&source)["format"]["start_time"]);
    assert!(
        source_start > 2.0,
        "the source must start late: {source_start}"
    );

    let (_temp, _root, previewer) = previewer();
    let mut info = info(128, 128, 3.0, "hevc", false);
    info.frame_rate = 10.0;
    previewer.open(source.clone(), info);
    let proxy = previewer.proxy(&source, ffmpeg_path, |_| {}).unwrap();

    // The player's clock starts at the proxy's first timestamp, which must be 0.
    let proxy_start = seconds(&ffprobe(&proxy)["format"]["start_time"]);
    assert!(proxy_start.abs() < 0.01, "proxy starts at {proxy_start}");
    for frame in [0_u32, 7, 13, 22, 29] {
        // Halfway into the frame before, so `-ss` lands on `frame` without rounding doubts.
        let at = (f64::from(frame) - 0.5).max(0.0) / 10.0;
        let expected = (f64::from(frame) * 7.0 * 255.0 / 219.0).round() as u8;
        let (in_source, in_proxy) = (luma_at(&source, at), luma_at(&proxy, at));
        assert!(
            in_source.abs_diff(expected) <= 3 && in_proxy.abs_diff(expected) <= 3,
            "at {at}s expected frame {frame} ({expected}): source {in_source}, proxy {in_proxy}"
        );
    }
}

#[test]
fn opening_another_source_cancels_a_running_build() {
    let clips = TempDir::new().unwrap();
    let first = long_clip(clips.path(), "first.mp4");
    let second = clips.path().join("second.mp4");
    std::fs::copy(&first, &second).unwrap();
    let (_temp, _root, previewer) = previewer();
    previewer.open(first.clone(), info(1920, 1080, 20.0, "hevc", false));

    let (started, running) = mpsc::channel();
    thread::scope(|scope| {
        let build = scope.spawn(|| {
            previewer.proxy(&first, ffmpeg_path, move |_| {
                let _ = started.send(());
            })
        });
        running
            .recv_timeout(Duration::from_secs(30))
            .expect("the build never reported progress");
        assert_eq!(
            proxies_in(&previewer).len(),
            1,
            "a partial proxy is being written"
        );

        let cancelled_at = Instant::now();
        previewer.open(second.clone(), info(1920, 1080, 20.0, "hevc", false));
        assert_eq!(build.join().unwrap(), Err(PreviewError::Cancelled));
        assert!(cancelled_at.elapsed() < Duration::from_secs(5));
    });

    assert_eq!(proxies_in(&previewer), Vec::<PathBuf>::new());
    assert_eq!(
        previewer.proxy(&first, ffmpeg_path, |_| {}),
        Err(PreviewError::NotOpen {
            path: first.display().to_string()
        })
    );
}

#[test]
fn a_proxy_is_reused_while_its_source_stays_open_and_deleted_when_replaced() {
    let clips = TempDir::new().unwrap();
    let first = hevc_mkv(clips.path());
    let second = clips.path().join("second.mkv");
    std::fs::copy(&first, &second).unwrap();
    let (_temp, _root, previewer) = previewer();
    let first_info = info(1440, 1080, 2.0, "hevc", true);

    previewer.open(first.clone(), first_info.clone());
    let proxy = previewer.proxy(&first, ffmpeg_path, |_| {}).unwrap();

    // Asking again, or reopening the same file, finds the proxy without running ffmpeg.
    previewer.open(first.clone(), first_info);
    let no_ffmpeg = || -> Option<PathBuf> { panic!("a reused proxy must not run ffmpeg") };
    let again = previewer.proxy(&first, no_ffmpeg, |_| {}).unwrap();
    assert_eq!(again, proxy);
    assert_eq!(proxies_in(&previewer), vec![proxy.clone()]);

    previewer.open(second, info(1440, 1080, 2.0, "hevc", true));
    assert!(
        !proxy.exists(),
        "the replaced source's proxy must be deleted"
    );
    assert_eq!(proxies_in(&previewer), Vec::<PathBuf>::new());
}

#[test]
fn shutting_down_stops_the_build_and_leaves_no_temp_folder() {
    let clips = TempDir::new().unwrap();
    let source = long_clip(clips.path(), "long.mp4");
    let (_temp, root, previewer) = previewer();
    previewer.open(source.clone(), info(1920, 1080, 20.0, "hevc", false));

    let (started, running) = mpsc::channel();
    thread::scope(|scope| {
        let build = scope.spawn(|| {
            previewer.proxy(&source, ffmpeg_path, move |_| {
                let _ = started.send(());
            })
        });
        running
            .recv_timeout(Duration::from_secs(30))
            .expect("the build never reported progress");

        previewer.shut_down().unwrap();
        assert!(!root.exists(), "the temp folder must be gone");
        assert_eq!(build.join().unwrap(), Err(PreviewError::Cancelled));
    });
    assert!(!root.exists());
    assert_eq!(
        previewer.proxy(&source, ffmpeg_path, |_| {}),
        Err(PreviewError::Cancelled)
    );
}

#[test]
fn a_missing_ffmpeg_is_reported() {
    let clips = TempDir::new().unwrap();
    let source = clips.path().join("any.mkv");
    std::fs::write(&source, b"").unwrap();
    let (_temp, _root, previewer) = previewer();
    previewer.open(source.clone(), info(320, 240, 1.0, "hevc", false));
    assert_eq!(
        previewer.proxy(&source, || None, |_| {}),
        Err(PreviewError::FfmpegMissing)
    );
}

#[test]
fn a_source_ffmpeg_cannot_read_fails_with_its_reason() {
    let clips = TempDir::new().unwrap();
    let source = clips.path().join("broken.mkv");
    std::fs::write(&source, b"not a video").unwrap();
    let (_temp, _root, previewer) = previewer();
    previewer.open(source.clone(), info(320, 240, 1.0, "hevc", false));

    match previewer.proxy(&source, ffmpeg_path, |_| {}) {
        Err(PreviewError::ProxyFailed { reason }) => assert!(!reason.is_empty()),
        other => panic!("expected ProxyFailed, got {other:?}"),
    }
    assert_eq!(proxies_in(&previewer), Vec::<PathBuf>::new());
}

#[test]
fn thumbnails_are_a_strip_of_24_frames_72_high_kept_until_the_source_is_replaced() {
    let clips = TempDir::new().unwrap();
    let first = long_clip(clips.path(), "first.mp4");
    let second = clips.path().join("second.mp4");
    std::fs::copy(&first, &second).unwrap();
    let (_temp, _root, previewer) = previewer();
    previewer.open(first.clone(), info(1920, 1080, 20.0, "h264", false));

    let thumbnails = previewer.thumbnails(&first, None, ffmpeg_path).unwrap();
    assert_eq!(thumbnails.len(), 24);
    for thumbnail in &thumbnails {
        assert!(thumbnail.starts_with(previewer.proxy_dir()));
        let video = &ffprobe(thumbnail)["streams"][0];
        assert_eq!(
            (video["width"].as_u64(), video["height"].as_u64()),
            (Some(128), Some(72))
        );
    }
    // testsrc2 changes every frame, so a strip of one repeated frame would show here.
    assert_ne!(
        std::fs::read(&thumbnails[0]).unwrap(),
        std::fs::read(&thumbnails[23]).unwrap()
    );

    let no_ffmpeg = || -> Option<PathBuf> { panic!("kept thumbnails must not run ffmpeg") };
    assert_eq!(
        previewer.thumbnails(&first, None, no_ffmpeg).unwrap(),
        thumbnails
    );
    assert_eq!(
        previewer.thumbnails(&second, None, ffmpeg_path),
        Err(PreviewError::NotOpen {
            path: second.display().to_string()
        })
    );

    previewer.open(second, info(1920, 1080, 20.0, "h264", false));
    assert!(
        !thumbnails[0].parent().unwrap().exists(),
        "the replaced source's thumbnails must be deleted"
    );
}

#[test]
fn closing_the_source_mid_build_stops_ffmpeg_and_leaves_no_proxy() {
    let clips = TempDir::new().unwrap();
    let source = long_clip(clips.path(), "long.mp4");
    let (_temp, _root, previewer) = previewer();
    previewer.open(source.clone(), info(1920, 1080, 20.0, "hevc", false));

    let (started, running) = mpsc::channel();
    thread::scope(|scope| {
        let build = scope.spawn(|| {
            previewer.proxy(&source, ffmpeg_path, move |_| {
                let _ = started.send(());
            })
        });
        running
            .recv_timeout(Duration::from_secs(30))
            .expect("the build never reported progress");
        assert_eq!(
            proxies_in(&previewer).len(),
            1,
            "a partial proxy is being written"
        );

        let closed_at = Instant::now();
        previewer.close(&source);
        assert_eq!(build.join().unwrap(), Err(PreviewError::Cancelled));
        assert!(closed_at.elapsed() < Duration::from_secs(5));
    });

    // Windows cannot delete a file ffmpeg still has open, so an empty folder means it exited.
    assert_eq!(proxies_in(&previewer), Vec::<PathBuf>::new());
    assert_eq!(
        previewer.proxy(&source, ffmpeg_path, |_| {}),
        Err(PreviewError::NotOpen {
            path: source.display().to_string()
        })
    );
}

#[test]
fn closing_the_source_deletes_its_proxy_and_thumbnails() {
    let clips = TempDir::new().unwrap();
    let source = hevc_mkv(clips.path());
    let (_temp, _root, previewer) = previewer();
    previewer.open(source.clone(), info(1440, 1080, 2.0, "hevc", true));
    let proxy = previewer.proxy(&source, ffmpeg_path, |_| {}).unwrap();
    let thumbnails = previewer.thumbnails(&source, None, ffmpeg_path).unwrap();
    assert_eq!(
        proxies_in(&previewer).len(),
        2,
        "the proxy and the thumbnail folder"
    );

    previewer.close(&source);

    assert!(!proxy.exists());
    assert!(!thumbnails[0].parent().unwrap().exists());
    assert_eq!(proxies_in(&previewer), Vec::<PathBuf>::new());
    assert!(source.exists(), "closing never touches the source");
}

#[test]
fn closing_a_source_that_is_not_open_leaves_the_open_one_alone() {
    let clips = TempDir::new().unwrap();
    let open = hevc_mkv(clips.path());
    let replaced = clips.path().join("replaced.mkv");
    std::fs::copy(&open, &replaced).unwrap();
    let (_temp, _root, previewer) = previewer();
    previewer.open(open.clone(), info(1440, 1080, 2.0, "hevc", true));
    let proxy = previewer.proxy(&open, ffmpeg_path, |_| {}).unwrap();

    previewer.close(&replaced);

    assert!(proxy.exists());
    let no_ffmpeg = || -> Option<PathBuf> { panic!("the kept proxy must not be rebuilt") };
    assert_eq!(previewer.proxy(&open, no_ffmpeg, |_| {}).unwrap(), proxy);
}

/// Grey frames whose brightness counts the frame modulo 25: frame n has luma 16 + 8 (n mod 25),
/// at 10 fps for 20 s, lossless, so a thumbnail says which frame it shows.
fn counting_clip(dir: &Path) -> PathBuf {
    generate(
        dir,
        "counting.mkv",
        &[
            "-f",
            "lavfi",
            "-i",
            r"nullsrc=size=128x72:rate=10:duration=20,geq=lum=16+8*mod(N\,25):cb=128:cr=128",
            "-c:v",
            "ffv1",
        ],
    )
}

/// The mean brightness of the image at `file`, decoded to full-range grey.
fn grey(file: &Path) -> f64 {
    let output = process::command(&ffmpeg_path().unwrap())
        .args(["-v", "error", "-i"])
        .arg(file)
        .args(["-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .unwrap();
    assert!(output.status.success(), "ffmpeg could not decode {file:?}");
    output.stdout.iter().map(|&v| f64::from(v)).sum::<f64>() / output.stdout.len() as f64
}

/// The grey [`counting_clip`]'s frame `n` decodes to: its luma expanded from video range.
fn counted(n: u32) -> f64 {
    f64::from(8 * (n % 25)) * 255.0 / 219.0
}

#[test]
fn a_range_strip_shows_the_frame_at_each_slice_middle_and_only_the_latest_are_kept() {
    let clips = TempDir::new().unwrap();
    let source = counting_clip(clips.path());
    let (_temp, _root, previewer) = previewer();
    previewer.open(source.clone(), info(128, 72, 20.0, "ffv1", false));
    let whole = previewer.thumbnails(&source, None, ffmpeg_path).unwrap();

    // 2.4 s from 12 s: a slice is one frame at 10 fps, and input -ss keeps the first frame at or
    // after each slice's middle (12.05 s onwards), so thumbnail i is frame 121 + i.
    let range = |start: f64, end: f64| Some(TimeRange { start, end });
    let strip = previewer
        .thumbnails(&source, range(12.0, 14.4), ffmpeg_path)
        .unwrap();
    assert_eq!(strip.len(), 24);
    for (i, thumbnail) in strip.iter().enumerate() {
        let shown = grey(thumbnail);
        let expected = counted(121 + i as u32);
        assert!(
            (shown - expected).abs() < 4.0,
            "thumbnail {i} has grey {shown:.1}, frame {} has {expected:.1}",
            121 + i
        );
    }

    let mut folders = vec![strip[0].parent().unwrap().to_owned()];
    for start in [1.0, 2.0, 3.0, 4.0] {
        let strip = previewer
            .thumbnails(&source, range(start, start + 2.4), ffmpeg_path)
            .unwrap();
        folders.push(strip[0].parent().unwrap().to_owned());
    }
    assert!(!folders[0].exists(), "the oldest range strip is deleted");
    assert!(folders[1..].iter().all(|folder| folder.exists()));
    assert!(
        whole[0].exists(),
        "range strips never replace the whole strip"
    );
    let no_ffmpeg = || -> Option<PathBuf> { panic!("the whole strip is kept") };
    assert_eq!(
        previewer.thumbnails(&source, None, no_ffmpeg).unwrap(),
        whole
    );
    assert_eq!(
        previewer
            .thumbnails(&source, range(0.0, 20.0), no_ffmpeg)
            .unwrap(),
        whole,
        "a range covering the whole source is the whole strip"
    );
    assert!(matches!(
        previewer.thumbnails(&source, range(25.0, 26.0), ffmpeg_path),
        Err(PreviewError::InvalidRange { .. })
    ));

    previewer.close(&source);
    assert!(folders.iter().all(|folder| !folder.exists()));
    assert_eq!(proxies_in(&previewer), Vec::<PathBuf>::new());
}

/// Range strips on a long recording, at the depth a zoomed timeline asks for. Run it by hand with
/// ffmpeg and ffprobe on PATH:
///
/// ```text
/// VTG_LONG_CLIP="C:\long.mp4" cargo test --release range_strips_on_a_long_recording -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a long recording in VTG_LONG_CLIP; run by hand"]
fn range_strips_on_a_long_recording() {
    let clip = PathBuf::from(std::env::var("VTG_LONG_CLIP").expect("set VTG_LONG_CLIP"));
    let info = crate::probe::infrastructure::ffprobe::probe(&ffprobe_path(), &clip).unwrap();
    assert!(
        info.duration > 30.0 * 60.0,
        "the clip must run past 30 minutes"
    );
    let (_temp, _root, previewer) = previewer();
    previewer.open(clip.clone(), info.clone());

    let started = Instant::now();
    let whole = previewer.thumbnails(&clip, None, ffmpeg_path).unwrap();
    println!(
        "whole {:.0} s: {:.2} s",
        info.duration,
        started.elapsed().as_secs_f64()
    );
    assert_eq!(whole.len(), 24);

    // From the whole source down to the deepest zoom, 48 frames wide, at three quarters through.
    let frame = 1.0 / info.frame_rate;
    let middle = info.duration * 0.75;
    let mut length = info.duration / 2.0;
    let mut previous: Option<Vec<u8>> = None;
    while length >= 48.0 * frame {
        let range = TimeRange {
            start: middle - length / 2.0,
            end: middle + length / 2.0,
        };
        let started = Instant::now();
        let strip = previewer
            .thumbnails(&clip, Some(range), ffmpeg_path)
            .unwrap();
        let took = started.elapsed().as_secs_f64();
        println!("range {length:.3} s: {took:.2} s");
        assert_eq!(strip.len(), 24);
        let video = &ffprobe(&strip[0])["streams"][0];
        assert_eq!(video["height"].as_u64(), Some(72));
        let first = std::fs::read(&strip[0]).unwrap();
        assert_ne!(
            previous.as_ref(),
            Some(&first),
            "each zoom shows other frames"
        );
        previous = Some(first);
        length /= 4.0;
    }
    previewer.close(&clip);
}

#[test]
fn a_strip_longer_than_90_seconds_seeks_a_keyframe_per_slice() {
    let clips = TempDir::new().unwrap();
    let source = generate(
        clips.path(),
        "two-minutes.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=10:duration=120",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "10",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    let (_temp, _root, previewer) = previewer();
    previewer.open(source.clone(), info(320, 180, 120.0, "h264", false));

    let thumbnails = previewer.thumbnails(&source, None, ffmpeg_path).unwrap();
    assert_eq!(thumbnails.len(), 24);
    for thumbnail in &thumbnails {
        let video = &ffprobe(thumbnail)["streams"][0];
        assert_eq!(
            (video["width"].as_u64(), video["height"].as_u64()),
            (Some(128), Some(72))
        );
    }
    assert_ne!(
        std::fs::read(&thumbnails[0]).unwrap(),
        std::fs::read(&thumbnails[23]).unwrap()
    );
}

#[test]
fn slices_past_the_last_keyframe_show_that_keyframe() {
    // Keyframes at 0, 30, 60 and 90 s; the last six slice middles, 92.5 s to 117.5 s, follow
    // the last one, as in AV1 encodes that place keyframes only at scene cuts.
    let clips = TempDir::new().unwrap();
    let source = generate(
        clips.path(),
        "sparse-keyframes.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=10:duration=120",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "300",
            "-keyint_min",
            "300",
            "-sc_threshold",
            "0",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    let (_temp, _root, previewer) = previewer();
    previewer.open(source.clone(), info(320, 180, 120.0, "h264", false));

    let thumbnails = previewer.thumbnails(&source, None, ffmpeg_path).unwrap();
    assert_eq!(thumbnails.len(), 24);
    let frame = |index: usize| std::fs::read(&thumbnails[index]).unwrap();
    assert_eq!(frame(18), frame(23), "both show the keyframe at 90 s");
    assert_ne!(frame(17), frame(18));
}

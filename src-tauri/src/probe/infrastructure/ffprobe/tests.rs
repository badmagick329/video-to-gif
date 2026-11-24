//! Probing clips that the real ffmpeg on PATH generates at test time (no media is committed),
//! plus validation cases that only hand-written ffprobe output can produce.

use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::{parse, probe};
use crate::probe::domain::{ProbeError, SourceInfo};
use crate::process;
use crate::search_path::SearchPath;

const VIDEO: [&str; 4] = [
    "-f",
    "lavfi",
    "-i",
    "testsrc2=size=320x240:rate=10:duration=1",
];
const AUDIO: [&str; 4] = ["-f", "lavfi", "-i", "sine=frequency=440:duration=1"];

fn tools() -> (PathBuf, PathBuf) {
    let located = SearchPath::from_env().locate_tools();
    (
        located
            .ffmpeg
            .expect("ffmpeg must be on PATH for the probe tests"),
        located
            .ffprobe
            .expect("ffprobe must be on PATH for the probe tests"),
    )
}

/// Runs `ffmpeg -y <args> <dir/name>` and returns the output path.
fn ffmpeg(dir: &TempDir, name: &str, args: &[&str]) -> PathBuf {
    let out = dir.path().join(name);
    let output = process::command(&tools().0)
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

fn probe_ok(file: &Path) -> SourceInfo {
    probe(&tools().1, file).unwrap_or_else(|err| panic!("probe failed: {err}"))
}

/// The fields every generated clip shares: 320x240 coded, 10 fps, about one second.
fn assert_clip(info: &SourceInfo, size: (u32, u32), codec: &str, container: &str, audio: bool) {
    assert_eq!((info.width, info.height), size, "{info:?}");
    assert_eq!(info.video_codec, codec, "{info:?}");
    assert!(
        info.container.split(',').any(|name| name == container),
        "{info:?}"
    );
    assert_eq!(info.has_audio, audio, "{info:?}");
    assert!((info.frame_rate - 10.0).abs() < 0.01, "{info:?}");
    assert!((0.9..=1.2).contains(&info.duration), "{info:?}");
}

fn with_audio<'a>(codec_args: &[&'a str]) -> Vec<&'a str> {
    [&VIDEO[..], &AUDIO[..], codec_args, &["-shortest"]].concat()
}

#[test]
fn h264_mp4() {
    let dir = TempDir::new().unwrap();
    let clip = ffmpeg(
        &dir,
        "h264.mp4",
        &with_audio(&["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"]),
    );
    assert_clip(&probe_ok(&clip), (320, 240), "h264", "mp4", true);
}

#[test]
fn av1_mp4() {
    let dir = TempDir::new().unwrap();
    let clip = ffmpeg(
        &dir,
        "av1.mp4",
        &with_audio(&["-c:v", "libsvtav1", "-preset", "12", "-c:a", "aac"]),
    );
    assert_clip(&probe_ok(&clip), (320, 240), "av1", "mp4", true);
}

#[test]
fn hevc_mkv() {
    let dir = TempDir::new().unwrap();
    let clip = ffmpeg(
        &dir,
        "hevc.mkv",
        &with_audio(&[
            "-c:v",
            "libx265",
            "-preset",
            "ultrafast",
            "-x265-params",
            "log-level=error",
            "-c:a",
            "aac",
        ]),
    );
    assert_clip(&probe_ok(&clip), (320, 240), "hevc", "matroska", true);
}

#[test]
fn vp9_webm() {
    let dir = TempDir::new().unwrap();
    let clip = ffmpeg(
        &dir,
        "vp9.webm",
        &with_audio(&[
            "-c:v",
            "libvpx-vp9",
            "-deadline",
            "realtime",
            "-cpu-used",
            "8",
            "-c:a",
            "libopus",
        ]),
    );
    assert_clip(&probe_ok(&clip), (320, 240), "vp9", "webm", true);
}

#[test]
fn clip_without_audio() {
    let dir = TempDir::new().unwrap();
    let args = [&VIDEO[..], &["-c:v", "libx264", "-pix_fmt", "yuv420p"]].concat();
    let clip = ffmpeg(&dir, "silent.mp4", &args);
    assert_clip(&probe_ok(&clip), (320, 240), "h264", "mp4", false);
}

#[test]
fn quarter_turn_rotation_swaps_the_displayed_size() {
    let dir = TempDir::new().unwrap();
    let args = [&VIDEO[..], &["-c:v", "libx264", "-pix_fmt", "yuv420p"]].concat();
    let upright = ffmpeg(&dir, "upright.mp4", &args);
    // `-display_rotation` is an input option: it attaches a display matrix to the remuxed
    // stream, as a phone does to a portrait recording.
    let upright = upright.to_str().unwrap();
    let rotated = ffmpeg(
        &dir,
        "rotated.mp4",
        &["-display_rotation", "90", "-i", upright, "-c", "copy"],
    );
    assert_clip(&probe_ok(&rotated), (240, 320), "h264", "mp4", false);
}

#[test]
fn audio_only_file_has_no_video_stream() {
    let dir = TempDir::new().unwrap();
    let args = [&AUDIO[..], &["-c:a", "aac"]].concat();
    let clip = ffmpeg(&dir, "tone.m4a", &args);
    assert!(matches!(
        probe(&tools().1, &clip),
        Err(ProbeError::NoVideoStream { .. })
    ));
}

#[test]
fn a_file_that_is_not_media_is_unreadable() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("notes.mp4");
    std::fs::write(&file, b"this is not a video").unwrap();
    match probe(&tools().1, &file) {
        Err(ProbeError::Unreadable { reason, .. }) => {
            assert!(!reason.is_empty());
            assert!(!reason.contains(dir.path().to_str().unwrap()), "{reason}");
        }
        other => panic!("expected Unreadable, got {other:?}"),
    }
}

// Validation of output the real ffprobe does not produce on demand.

fn parse_json(value: serde_json::Value) -> Result<SourceInfo, ProbeError> {
    parse(value.to_string().as_bytes(), Path::new("C:\\clip.mp4"))
}

fn video_stream() -> serde_json::Value {
    json!({
        "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
        "avg_frame_rate": "30000/1001", "r_frame_rate": "30000/1001"
    })
}

fn output_with(stream: serde_json::Value) -> serde_json::Value {
    json!({
        "streams": [stream],
        "format": { "format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "12.5" }
    })
}

fn assert_invalid(result: Result<SourceInfo, ProbeError>) {
    assert!(
        matches!(result, Err(ProbeError::InvalidOutput { .. })),
        "{result:?}"
    );
}

#[test]
fn legacy_rotate_tag_swaps_the_displayed_size() {
    let mut stream = video_stream();
    stream["tags"] = json!({ "rotate": "270" });
    let info = parse_json(output_with(stream)).unwrap();
    assert_eq!((info.width, info.height), (1080, 1920));
    assert!((info.frame_rate - 29.97).abs() < 0.01);
    assert_eq!(info.duration, 12.5);
}

#[test]
fn cover_art_is_not_the_video_stream() {
    let cover = json!({
        "codec_type": "video", "codec_name": "mjpeg", "width": 600, "height": 600,
        "avg_frame_rate": "0/0", "r_frame_rate": "90000/1", "disposition": { "attached_pic": 1 }
    });
    let audio = json!({ "codec_type": "audio", "codec_name": "mp3" });

    let audio_with_cover = json!({
        "streams": [cover, audio],
        "format": { "format_name": "mp3", "duration": "200.0" }
    });
    assert!(matches!(
        parse_json(audio_with_cover),
        Err(ProbeError::NoVideoStream { .. })
    ));

    let video_with_cover = json!({
        "streams": [cover, video_stream(), audio],
        "format": { "format_name": "matroska,webm", "duration": "3.0" }
    });
    let info = parse_json(video_with_cover).unwrap();
    assert_eq!((info.video_codec.as_str(), info.width), ("h264", 1920));
    assert!(info.has_audio);
}

#[test]
fn unknown_average_rate_falls_back_to_the_real_base_rate() {
    let mut stream = video_stream();
    stream["avg_frame_rate"] = json!("0/0");
    stream["r_frame_rate"] = json!("25/1");
    assert_eq!(parse_json(output_with(stream)).unwrap().frame_rate, 25.0);
}

#[test]
fn implausible_frame_rates_are_rejected() {
    let mut stream = video_stream();
    stream["avg_frame_rate"] = json!("0/0");
    stream["r_frame_rate"] = json!("90000/1");
    assert_invalid(parse_json(output_with(stream)));
}

#[test]
fn missing_or_zero_dimensions_are_rejected() {
    let mut stream = video_stream();
    stream.as_object_mut().unwrap().remove("width");
    assert_invalid(parse_json(output_with(stream)));

    let mut stream = video_stream();
    stream["height"] = json!(0);
    assert_invalid(parse_json(output_with(stream)));
}

#[test]
fn missing_or_non_positive_duration_is_rejected() {
    let mut output = output_with(video_stream());
    output["format"]["duration"] = json!("0.000000");
    assert_invalid(parse_json(output));

    let mut output = output_with(video_stream());
    output["format"].as_object_mut().unwrap().remove("duration");
    assert_invalid(parse_json(output));
}

#[test]
fn stream_duration_stands_in_for_a_missing_format_duration() {
    let mut stream = video_stream();
    stream["duration"] = json!("4.2");
    let mut output = output_with(stream);
    output["format"].as_object_mut().unwrap().remove("duration");
    assert_eq!(parse_json(output).unwrap().duration, 4.2);
}

#[test]
fn output_that_is_not_json_is_rejected() {
    assert_invalid(parse(b"<html>", Path::new("C:\\clip.mp4")));
}

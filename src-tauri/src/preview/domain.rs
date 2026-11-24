//! The preview slice's rules, free of I/O: which sources WebView2 plays as they are, the proxy's
//! size and ffmpeg arguments, and reading ffmpeg's progress output.

use std::ffi::OsString;
use std::fmt;
use std::path::Path;

use serde::Serialize;

use crate::probe::domain::SourceInfo;

/// What the player plays for an opened source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewSource {
    /// The source file itself, served to the webview in place.
    Source,
    /// A proxy that ffmpeg makes first.
    Proxy,
}

/// Video codecs (ffprobe `codec_name`) played from the source, each confirmed by playing a
/// generated clip in the app's WebView2 on Windows 11. HEVC is left out on purpose: WebView2
/// decodes it only where the HEVC Video Extensions and a capable GPU are present (the owner's
/// machine has both), so whether it plays depends on the user's PC rather than the file.
const PLAYABLE_CODECS: [&str; 3] = ["h264", "vp9", "av1"];

/// Plays the source itself when it is H.264, VP9 or AV1 in MP4 or WebM; anything else gets a
/// proxy. The frontend still falls back to a proxy if the player reports an error on the source,
/// or shows no picture, which catches files the allowlist admits but WebView2 rejects (e.g. 10-bit
/// or 4:4:4 H.264).
///
/// ffprobe reports MKV and WebM alike (`format_name` `matroska,webm`; only the file's EBML DocType
/// differs), so a Matroska file counts as WebM only when `source` is named `.webm`. MKV files with
/// these codecs also played in WebView2, but MKV carries codecs and features (FLAC, PCM, ordered
/// chapters, styled subtitles) WebM forbids, so MKV takes the proxy. MP4's family (`mov,mp4,...`)
/// also covers MOV, whose H.264 Chromium reads with the same demuxer.
pub fn choose_preview_source(info: &SourceInfo, source: &Path) -> PreviewSource {
    let codec_ok = PLAYABLE_CODECS.contains(&info.video_codec.as_str());
    let demuxers = || info.container.split(',').map(str::trim);
    let is_webm = source
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("webm"));
    let container_ok =
        demuxers().any(|name| name == "mp4") || (is_webm && demuxers().any(|name| name == "webm"));
    if codec_ok && container_ok {
        PreviewSource::Source
    } else {
        PreviewSource::Proxy
    }
}

/// Tallest proxy, in pixels. Preview only needs to show enough to trim and crop by, and a smaller
/// proxy is faster to make.
pub const MAX_PROXY_HEIGHT: u32 = 720;

/// The proxy's frame size for a source of displayed size `width`×`height`: height capped at
/// [`MAX_PROXY_HEIGHT`] and never upscaled, width keeping the aspect, both even because x264's
/// 4:2:0 output needs even dimensions.
pub fn proxy_size(width: u32, height: u32) -> (u32, u32) {
    let target_height = (height.min(MAX_PROXY_HEIGHT) & !1).max(2);
    let exact_width = f64::from(width) * f64::from(target_height) / f64::from(height);
    let target_width = ((exact_width / 2.0).round() as u32 * 2).max(2);
    (target_width, target_height)
}

/// The ffmpeg arguments that turn `source` into a proxy at `output`, with progress on stdout.
///
/// - `-map 0:V:0` picks the first video stream that is not cover art, the stream the probe
///   described; `-map 0:a:0` adds the first audio stream when there is one.
/// - No `-ss`, `-copyts` or `-r`: ffmpeg starts the output at 0 at the source's first timestamp
///   and `-fps_mode passthrough` keeps every frame's time, so time t in the proxy is time t into
///   the source, the same offset export's `-ss` measures.
/// - The scale uses the displayed size: ffmpeg auto-rotates before filters, and the scale filter
///   keeps the display aspect.
/// - x264 `ultrafast` is the quickest preset; a keyframe every second keeps seeking quick
///   whatever the frame rate. Audio goes to stereo AAC, which every layout downmixes to.
/// - `+faststart` puts the index first, so the player can start from the first range request.
pub fn proxy_args(source: &Path, info: &SourceInfo, output: &Path) -> Vec<OsString> {
    let (width, height) = proxy_size(info.width, info.height);
    let mut args: Vec<OsString> = [
        "-hide_banner",
        "-v",
        "error",
        "-nostats",
        "-progress",
        "pipe:1",
        "-y",
        "-i",
    ]
    .map(OsString::from)
    .into();
    args.push(source.into());
    args.extend(["-map", "0:V:0"].map(OsString::from));
    if info.has_audio {
        args.extend(["-map", "0:a:0"].map(OsString::from));
    }
    args.extend(
        [
            "-vf".to_owned(),
            format!("scale={width}:{height},format=yuv420p"),
            "-fps_mode".to_owned(),
            "passthrough".to_owned(),
            "-c:v".to_owned(),
            "libx264".to_owned(),
            "-preset".to_owned(),
            "ultrafast".to_owned(),
            "-crf".to_owned(),
            "23".to_owned(),
            "-force_key_frames".to_owned(),
            "expr:gte(t,n_forced)".to_owned(),
        ]
        .map(OsString::from),
    );
    if info.has_audio {
        args.extend(["-c:a", "aac", "-b:a", "128k", "-ac", "2"].map(OsString::from));
    }
    args.extend(["-movflags", "+faststart", "-f", "mp4"].map(OsString::from));
    args.push(output.into());
    args
}

/// How far a proxy build has got, from one line of ffmpeg's `-progress` output. `duration` is the
/// source's, in seconds. Returns `None` for lines that carry no position.
///
/// ffmpeg writes `key=value` lines; `out_time_us` is the output position in microseconds (`N/A`
/// before the first frame), and `progress=end` closes the run.
pub fn progress_from_line(line: &str, duration: f64) -> Option<f64> {
    let (key, value) = line.trim().split_once('=')?;
    match key {
        "progress" if value == "end" => Some(1.0),
        "out_time_us" => {
            let micros: f64 = value.parse().ok()?;
            Some((micros / 1_000_000.0 / duration).clamp(0.0, 1.0))
        }
        _ => None,
    }
}

/// Frames in the trim timeline's thumbnail strip, mgck's `timelineThumbnailCount`.
pub const THUMBNAIL_COUNT: u32 = 24;

/// Height of each thumbnail, in pixels: mgck draws them 72 high.
pub const THUMBNAIL_HEIGHT: u32 = 72;

/// Past this many seconds, thumbnails come from keyframes only (gif-studio's `makeStrip`).
const KEYFRAMES_ONLY_AFTER: f64 = 90.0;

/// The ffmpeg arguments that write the timeline's [`THUMBNAIL_COUNT`] thumbnails of `input`, a
/// preview source `duration` seconds long, into `dir` as `thumb-01.jpg` onwards, in time order.
///
/// One ffmpeg pass samples the middle of each of the strip's equal slices (mgck seeks the
/// `<video>` to the same times): the input `-ss` skips half a slice and `fps` takes one frame a
/// slice from there. `tpad` repeats the last frame so the strip always fills: without it, and
/// `eof_action=pass`, `fps` stops at the last decoded frame, one short on any clip and several
/// short when only sparse keyframes are decoded. Seeking a hidden `<video>` instead took 20 s for a 2-minute 4K H.264 MP4
/// with a keyframe every 10 s, where this pass takes under a second.
///
/// Like gif-studio, a source longer than [`KEYFRAMES_ONLY_AFTER`] is decoded at keyframes only
/// (`-skip_frame nokey`), so a long 4K file costs a read rather than a full decode; each
/// thumbnail is then the last keyframe before its time, which a strip can afford. A proxy has a
/// keyframe every second.
pub fn thumbnail_args(input: &Path, duration: f64, dir: &Path) -> Vec<OsString> {
    let slice = duration / f64::from(THUMBNAIL_COUNT);
    let mut args: Vec<OsString> = ["-hide_banner", "-v", "error", "-nostats", "-y"]
        .map(OsString::from)
        .into();
    if duration > KEYFRAMES_ONLY_AFTER {
        args.extend(["-skip_frame", "nokey"].map(OsString::from));
    }
    args.extend(
        [
            "-ss".to_owned(),
            format!("{:.3}", slice / 2.0),
            "-i".to_owned(),
        ]
        .map(OsString::from),
    );
    args.push(input.into());
    args.extend(
        [
            "-map".to_owned(),
            "0:V:0".to_owned(),
            "-an".to_owned(),
            "-vf".to_owned(),
            format!(
                "tpad=stop_mode=clone:stop_duration={duration:.3},fps=1/{slice:.6}:eof_action=pass,scale=-2:{THUMBNAIL_HEIGHT}"
            ),
            "-frames:v".to_owned(),
            THUMBNAIL_COUNT.to_string(),
            "-q:v".to_owned(),
            "5".to_owned(),
            "-f".to_owned(),
            "image2".to_owned(),
        ]
        .map(OsString::from),
    );
    args.push(dir.join("thumb-%02d.jpg").into());
    args
}

/// Progress of a proxy build, emitted to the frontend as the `proxy-progress` event.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProgress {
    /// The source the proxy is for, so the frontend can drop events for one it has replaced.
    pub source: String,
    /// From 0 to 1.
    pub progress: f64,
}

/// Why opening a source for preview or making its proxy failed. Serialized with a `kind` tag so
/// the frontend can word each case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PreviewError {
    /// The path is not an absolute path to an existing file.
    NotFound { path: String },
    /// The source info sent with the path is impossible (zero size, no duration).
    InvalidSourceInfo { reason: String },
    /// The webview could not be given access to the file.
    NotServable { path: String, reason: String },
    /// The path is not the source currently open, e.g. because another one replaced it.
    NotOpen { path: String },
    /// ffmpeg is not on PATH.
    FfmpegMissing,
    /// ffmpeg could not start or failed; `reason` is its last error line.
    ProxyFailed { reason: String },
    /// ffmpeg could not make the timeline thumbnails; `reason` is its last error line.
    ThumbnailsFailed { reason: String },
    /// The build was stopped because another source was opened or the app is closing.
    Cancelled,
}

impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreviewError::NotFound { path } => write!(f, "no file at {path}"),
            PreviewError::InvalidSourceInfo { reason } => {
                write!(f, "invalid source info: {reason}")
            }
            PreviewError::NotServable { path, reason } => {
                write!(f, "cannot serve {path} to the player: {reason}")
            }
            PreviewError::NotOpen { path } => write!(f, "{path} is not the open source"),
            PreviewError::FfmpegMissing => write!(f, "ffmpeg was not found on PATH"),
            PreviewError::ProxyFailed { reason } => write!(f, "proxy failed: {reason}"),
            PreviewError::ThumbnailsFailed { reason } => write!(f, "thumbnails failed: {reason}"),
            PreviewError::Cancelled => write!(f, "proxy build cancelled"),
        }
    }
}

impl std::error::Error for PreviewError {}

#[cfg(test)]
mod tests {
    use super::*;

    const MP4: &str = "mov,mp4,m4a,3gp,3g2,mj2";
    const MATROSKA: &str = "matroska,webm";

    fn info(codec: &str, container: &str) -> SourceInfo {
        SourceInfo {
            duration: 10.0,
            width: 1920,
            height: 1080,
            frame_rate: 30.0,
            video_codec: codec.to_owned(),
            container: container.to_owned(),
            has_audio: true,
        }
    }

    fn choose(codec: &str, container: &str, file: &str) -> PreviewSource {
        choose_preview_source(&info(codec, container), Path::new(file))
    }

    #[test]
    fn allowed_codecs_in_mp4_or_webm_play_the_source() {
        for codec in ["h264", "vp9", "av1"] {
            assert_eq!(choose(codec, MP4, "clip.mp4"), PreviewSource::Source);
            assert_eq!(choose(codec, MP4, "clip.MOV"), PreviewSource::Source);
        }
        for codec in ["vp9", "av1"] {
            assert_eq!(choose(codec, MATROSKA, "clip.WebM"), PreviewSource::Source);
        }
    }

    #[test]
    fn mkv_gets_a_proxy_whatever_its_codec() {
        for codec in ["h264", "vp9", "av1", "hevc"] {
            assert_eq!(choose(codec, MATROSKA, "clip.mkv"), PreviewSource::Proxy);
        }
    }

    #[test]
    fn hevc_always_gets_a_proxy() {
        assert_eq!(choose("hevc", MP4, "clip.mp4"), PreviewSource::Proxy);
        assert_eq!(choose("hevc", MATROSKA, "clip.webm"), PreviewSource::Proxy);
    }

    #[test]
    fn other_codecs_and_containers_get_a_proxy() {
        for (codec, container, file) in [
            ("mpeg4", MP4, "a.mp4"),
            ("prores", MP4, "a.mov"),
            ("h264", "avi", "a.avi"),
            ("h264", "mpegts", "a.ts"),
            ("vp9", "ogg", "a.webm"),
            ("av1", "flv", "a.flv"),
        ] {
            assert_eq!(
                choose(codec, container, file),
                PreviewSource::Proxy,
                "{codec} in {container}"
            );
        }
    }

    #[test]
    fn proxy_height_is_capped_and_aspect_kept() {
        assert_eq!(proxy_size(1920, 1080), (1280, 720));
        assert_eq!(proxy_size(3840, 2160), (1280, 720));
        assert_eq!(proxy_size(1080, 1920), (406, 720));
    }

    #[test]
    fn small_sources_are_not_upscaled() {
        assert_eq!(proxy_size(320, 240), (320, 240));
        assert_eq!(proxy_size(1280, 720), (1280, 720));
    }

    #[test]
    fn proxy_dimensions_are_even() {
        for (w, h) in [(321, 241), (1919, 1079), (853, 480), (3, 3), (1, 1)] {
            let (pw, ph) = proxy_size(w, h);
            assert!(pw % 2 == 0 && ph % 2 == 0, "{w}x{h} -> {pw}x{ph}");
            assert!(ph <= h.max(2) && pw >= 2, "{w}x{h} -> {pw}x{ph}");
        }
    }

    fn args_text(info: &SourceInfo) -> Vec<String> {
        proxy_args(
            Path::new("C:\\in.mkv"),
            info,
            Path::new("C:\\tmp\\proxy-1.mp4"),
        )
        .into_iter()
        .map(|arg| arg.into_string().unwrap())
        .collect()
    }

    fn has_pair(args: &[String], key: &str, value: &str) -> bool {
        args.windows(2)
            .any(|pair| pair[0] == key && pair[1] == value)
    }

    #[test]
    fn proxy_args_make_a_seekable_h264_mp4_with_progress() {
        let args = args_text(&info("hevc", MATROSKA));
        assert!(has_pair(&args, "-i", "C:\\in.mkv"));
        assert!(has_pair(&args, "-progress", "pipe:1"));
        assert!(has_pair(&args, "-c:v", "libx264"));
        assert!(has_pair(&args, "-vf", "scale=1280:720,format=yuv420p"));
        assert!(has_pair(&args, "-force_key_frames", "expr:gte(t,n_forced)"));
        assert!(has_pair(&args, "-c:a", "aac"));
        assert!(has_pair(&args, "-map", "0:a:0"));
        assert_eq!(args.last().unwrap(), "C:\\tmp\\proxy-1.mp4");
        // Anything that shifts or resamples time would break the source/proxy time mapping.
        for flag in ["-ss", "-copyts", "-r", "-itsoffset"] {
            assert!(!args.iter().any(|arg| arg == flag), "{flag} in {args:?}");
        }
    }

    #[test]
    fn proxy_args_leave_audio_out_for_a_silent_source() {
        let mut silent = info("hevc", MATROSKA);
        silent.has_audio = false;
        let args = args_text(&silent);
        assert!(!args.iter().any(|arg| arg == "-c:a" || arg == "0:a:0"));
    }

    #[test]
    fn thumbnails_sample_slice_middles_and_read_keyframes_only_past_90_seconds() {
        let text = |duration: f64| -> Vec<String> {
            thumbnail_args(
                Path::new("C:\\in.mp4"),
                duration,
                Path::new("C:\\tmp\\thumbs-1"),
            )
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
        };
        let short = text(48.0);
        assert!(has_pair(&short, "-ss", "1.000"));
        assert!(has_pair(
            &short,
            "-vf",
            "tpad=stop_mode=clone:stop_duration=48.000,fps=1/2.000000:eof_action=pass,scale=-2:72"
        ));
        assert!(has_pair(&short, "-frames:v", "24"));
        assert!(!short.iter().any(|arg| arg == "-skip_frame"));
        assert_eq!(short.last().unwrap(), "C:\\tmp\\thumbs-1\\thumb-%02d.jpg");
        assert!(has_pair(&text(187.0), "-skip_frame", "nokey"));
    }

    #[test]
    fn progress_is_output_time_over_duration() {
        assert_eq!(progress_from_line("out_time_us=2500000", 10.0), Some(0.25));
        assert_eq!(progress_from_line("out_time_us=N/A", 10.0), None);
        assert_eq!(progress_from_line("out_time_us=99000000", 10.0), Some(1.0));
        assert_eq!(progress_from_line("progress=continue", 10.0), None);
        assert_eq!(progress_from_line("progress=end", 10.0), Some(1.0));
        assert_eq!(progress_from_line("frame=12", 10.0), None);
    }
}

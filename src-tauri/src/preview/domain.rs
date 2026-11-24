//! The preview slice's rules, free of I/O: which sources WebView2 plays as they are, the proxy's
//! size and ffmpeg arguments, and reading ffmpeg's progress output.

use std::ffi::OsString;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

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
const PLAYABLE_VIDEO_CODECS: [&str; 3] = ["h264", "vp9", "av1"];

/// Audio codecs (ffprobe `codec_name`) played from the source, per container. Each pair was
/// confirmed on generated clips (testsrc2 with a sine tone) in Chrome and in the app's WebView2:
/// the tone reached Web Audio as non-silent samples and `webkitAudioDecodedByteCount` grew,
/// before and after a seek. The two lists match today; they stay separate because each pair was
/// tested on its own and a future codec may not hold in both.
///
/// Audio Chromium cannot decode plays as silence with no error event, so the frontend's fallback
/// cannot catch it; whatever is left out takes the proxy. DTS decoded nothing in either browser.
/// AC-3 and E-AC-3 decoded in WebView2 on the owner's machine but not in Chrome: Edge hands them
/// to the Windows Dolby decoder, which not every Windows install has, so like HEVC they depend on
/// the PC. PCM decoded in both but is outside the set the owner asked to admit.
const MP4_AUDIO_CODECS: [&str; 5] = ["aac", "mp3", "opus", "vorbis", "flac"];
const MATROSKA_AUDIO_CODECS: [&str; 5] = ["aac", "mp3", "opus", "vorbis", "flac"];

/// A container family the player reads, as far as the allowlist is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Container {
    /// ffprobe's `mov,mp4,...` family, which also covers MOV: Chromium reads both with one demuxer.
    Mp4,
    /// MKV and WebM.
    Matroska,
}

/// The container of `source`, if it is one the player reads.
///
/// ffprobe names MKV and WebM alike (`format_name` `matroska,webm`; only the EBML DocType
/// differs), and Chromium reads both with one Matroska demuxer: a WebM-named MKV holding FLAC
/// played like the MKV. So the extension does not pick a demuxer; it only limits direct play to the
/// names that were tested, `.mkv` and `.webm`. Any other name for a Matroska file (`.mka`, `.mk3d`,
/// a misnamed file) takes the proxy.
fn container(info: &SourceInfo, source: &Path) -> Option<Container> {
    let mut demuxers = info.container.split(',').map(str::trim);
    let has_extension = |wanted: &str| {
        source
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(wanted))
    };
    if demuxers.clone().any(|name| name == "mp4") {
        Some(Container::Mp4)
    } else if demuxers.any(|name| name == "matroska")
        && (has_extension("mkv") || has_extension("webm"))
    {
        Some(Container::Matroska)
    } else {
        None
    }
}

/// Plays the source itself when its video is H.264, VP9 or AV1, its container MP4 or Matroska
/// (`.mkv` or `.webm`), and its audio absent or a codec WebView2 decodes in that container; anything
/// else gets a proxy.
///
/// The frontend still falls back to a proxy if the player reports an error on the source, or shows
/// no picture, which catches files the allowlist admits but WebView2 rejects (e.g. 10-bit or 4:4:4
/// H.264). Undecodable audio raises neither, which is why the audio codec is checked here.
pub fn choose_preview_source(info: &SourceInfo, source: &Path) -> PreviewSource {
    let video_ok = PLAYABLE_VIDEO_CODECS.contains(&info.video_codec.as_str());
    let audio_ok = |playable: &[&str]| {
        info.audio_codec
            .as_deref()
            .is_none_or(|codec| playable.contains(&codec))
    };
    let container_ok = match container(info, source) {
        Some(Container::Mp4) => audio_ok(&MP4_AUDIO_CODECS),
        Some(Container::Matroska) => audio_ok(&MATROSKA_AUDIO_CODECS),
        None => false,
    };
    if video_ok && container_ok {
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
    if info.has_audio() {
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
    if info.has_audio() {
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

/// Frames in a strip of timeline thumbnails, whether it covers the whole source or a range.
pub const THUMBNAIL_COUNT: u32 = 24;

/// Height of each thumbnail, in pixels: the timeline dock's track is 72 high.
pub const THUMBNAIL_HEIGHT: u32 = 72;

/// Past this many seconds of strip, thumbnails are keyframes (gif-studio's `makeStrip` draws the
/// line at the same length): slices of 3.75 s or more, where the last keyframe before each
/// slice's middle stands in well enough.
const KEYFRAMES_ONLY_AFTER: f64 = 90.0;

/// A span of the source's timeline in seconds, as the frontend asks for thumbnails of it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct TimeRange {
    pub start: f64,
    pub end: f64,
}

/// The part of a source `duration` seconds long that a thumbnail strip should cover: `None` for
/// the whole source, which is also what a range covering all of it means, so that strip is made
/// once and kept. A range reaching past the end is cut there. A range that is not finite, starts
/// before 0 or at or after the end, or ends before it starts is rejected: the command cannot
/// assume the frontend sent a sane one.
pub fn thumbnail_span(
    range: Option<TimeRange>,
    duration: f64,
) -> Result<Option<TimeRange>, PreviewError> {
    let Some(range) = range else {
        return Ok(None);
    };
    let reason = if !(range.start.is_finite() && range.end.is_finite()) {
        "the range must be finite"
    } else if range.start < 0.0 || range.start >= duration {
        "the range must start inside the source"
    } else if range.end <= range.start {
        "the range must end after it starts"
    } else if range.start == 0.0 && range.end >= duration {
        return Ok(None);
    } else {
        return Ok(Some(TimeRange {
            start: range.start,
            end: range.end.min(duration),
        }));
    };
    Err(PreviewError::InvalidRange {
        reason: reason.to_owned(),
    })
}

/// The ffmpeg arguments that write [`THUMBNAIL_COUNT`] thumbnails of `input`, a preview source,
/// covering `length` seconds from `start`, into `dir` as `thumb-01.jpg` onwards, in time order.
/// Each samples the middle of one of the strip's equal slices, as mgck seeks its `<video>` to.
///
/// A strip up to [`KEYFRAMES_ONLY_AFTER`] long, such as the ranges a zoomed timeline asks for, is
/// one decoding pass: the input `-ss` skips to the first slice's middle and `fps` takes one frame
/// a slice from there, so each thumbnail is the first frame at or after its slice's middle.
/// `tpad` repeats the last frame so the strip always fills: without it, and `eof_action=pass`,
/// `fps` stops at the last decoded frame, one short on any clip. Seeking a hidden `<video>`
/// instead took 20 s for a 2-minute 4K H.264 MP4 with a keyframe every 10 s, where this pass
/// takes under a second.
///
/// A longer strip seeks instead of decoding: one input per slice, each opened at its slice's
/// middle without accurate seeking and decoding keyframes only, so each thumbnail is the last
/// keyframe before its time and ffmpeg reads 24 keyframes rather than the whole file. On a
/// 2-hour 1080p H.264 recording with a keyframe every 3 s that takes 3 s, where one
/// keyframes-only pass (gif-studio's way) took 44 s, since it still demuxes every packet.
pub fn thumbnail_args(input: &Path, start: f64, length: f64, dir: &Path) -> Vec<OsString> {
    let slice = length / f64::from(THUMBNAIL_COUNT);
    let mut args: Vec<OsString> = ["-hide_banner", "-v", "error", "-nostats", "-y"]
        .map(OsString::from)
        .into();
    if length > KEYFRAMES_ONLY_AFTER {
        for index in 0..THUMBNAIL_COUNT {
            let middle = start + slice * (f64::from(index) + 0.5);
            args.extend(
                [
                    "-skip_frame",
                    "nokey",
                    "-noaccurate_seek",
                    // One keyframe each; 24 decoders with a thread per core only cost startup.
                    "-threads",
                    "1",
                    "-ss",
                    &format!("{middle:.3}"),
                    "-i",
                ]
                .map(OsString::from),
            );
            args.push(input.into());
        }
        for index in 0..THUMBNAIL_COUNT {
            args.extend(
                [
                    "-map".to_owned(),
                    format!("{index}:V:0"),
                    "-frames:v".to_owned(),
                    "1".to_owned(),
                    "-vf".to_owned(),
                    format!("scale=-2:{THUMBNAIL_HEIGHT}"),
                    "-q:v".to_owned(),
                    "5".to_owned(),
                ]
                .map(OsString::from),
            );
            args.push(dir.join(format!("thumb-{:02}.jpg", index + 1)).into());
        }
        return args;
    }
    args.extend(
        [
            "-ss".to_owned(),
            format!("{:.3}", start + slice / 2.0),
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
                "tpad=stop_mode=clone:stop_duration={length:.3},fps=1/{slice:.6}:eof_action=pass,scale=-2:{THUMBNAIL_HEIGHT}"
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
    /// The time range asked for thumbnails is impossible (not finite, empty, outside the source).
    InvalidRange { reason: String },
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
            PreviewError::InvalidRange { reason } => write!(f, "invalid range: {reason}"),
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
            audio_codec: Some("aac".to_owned()),
        }
    }

    fn choose_with(video: &str, audio: Option<&str>, container: &str, file: &str) -> PreviewSource {
        let mut info = info(video, container);
        info.audio_codec = audio.map(str::to_owned);
        choose_preview_source(&info, Path::new(file))
    }

    fn choose(codec: &str, container: &str, file: &str) -> PreviewSource {
        choose_preview_source(&info(codec, container), Path::new(file))
    }

    /// Every pair confirmed in WebView2, with each video codec, and with no audio at all.
    #[test]
    fn confirmed_video_and_audio_in_mp4_or_matroska_play_the_source() {
        let audio = [
            None,
            Some("aac"),
            Some("mp3"),
            Some("opus"),
            Some("vorbis"),
            Some("flac"),
        ];
        for video in ["h264", "vp9", "av1"] {
            for audio in audio {
                for (container, file) in [
                    (MP4, "clip.mp4"),
                    (MP4, "clip.MOV"),
                    (MATROSKA, "clip.mkv"),
                    (MATROSKA, "clip.MKV"),
                    (MATROSKA, "clip.WebM"),
                ] {
                    assert_eq!(
                        choose_with(video, audio, container, file),
                        PreviewSource::Source,
                        "{video} + {audio:?} in {file}"
                    );
                }
            }
        }
    }

    /// The owner's 4K recording: H.264 with FLAC in MKV.
    #[test]
    fn h264_with_flac_in_mkv_plays_the_source() {
        assert_eq!(
            choose_with(
                "h264",
                Some("flac"),
                MATROSKA,
                "190422 Twice - Turn it up.mkv"
            ),
            PreviewSource::Source
        );
    }

    /// These decode to silence with no error event, which the frontend's fallback cannot see.
    #[test]
    fn audio_webview2_does_not_decode_gets_a_proxy() {
        for audio in [
            "ac3",
            "eac3",
            "dts",
            "truehd",
            "pcm_s16le",
            "alac",
            "unknown",
        ] {
            for (container, file) in [(MP4, "clip.mp4"), (MATROSKA, "clip.mkv")] {
                assert_eq!(
                    choose_with("h264", Some(audio), container, file),
                    PreviewSource::Proxy,
                    "{audio} in {file}"
                );
            }
        }
    }

    #[test]
    fn matroska_under_another_name_gets_a_proxy() {
        for file in ["clip.mka", "clip.mk3d", "clip.avi", "clip"] {
            assert_eq!(
                choose("h264", MATROSKA, file),
                PreviewSource::Proxy,
                "{file}"
            );
        }
    }

    #[test]
    fn hevc_always_gets_a_proxy() {
        assert_eq!(choose("hevc", MP4, "clip.mp4"), PreviewSource::Proxy);
        assert_eq!(choose("hevc", MATROSKA, "clip.mkv"), PreviewSource::Proxy);
        assert_eq!(choose("hevc", MATROSKA, "clip.webm"), PreviewSource::Proxy);
    }

    #[test]
    fn other_codecs_and_containers_get_a_proxy() {
        for (codec, container, file) in [
            ("mpeg4", MP4, "a.mp4"),
            ("prores", MP4, "a.mov"),
            ("mpeg2video", MATROSKA, "a.mkv"),
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
        silent.audio_codec = None;
        let args = args_text(&silent);
        assert!(!args.iter().any(|arg| arg == "-c:a" || arg == "0:a:0"));
    }

    #[test]
    fn thumbnails_sample_slice_middles_and_read_keyframes_only_past_90_seconds() {
        let text = |start: f64, length: f64| -> Vec<String> {
            thumbnail_args(
                Path::new("C:\\in.mp4"),
                start,
                length,
                Path::new("C:\\tmp\\thumbs-1"),
            )
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
        };
        let short = text(0.0, 48.0);
        assert!(has_pair(&short, "-ss", "1.000"));
        assert!(has_pair(
            &short,
            "-vf",
            "tpad=stop_mode=clone:stop_duration=48.000,fps=1/2.000000:eof_action=pass,scale=-2:72"
        ));
        assert!(has_pair(&short, "-frames:v", "24"));
        assert!(!short.iter().any(|arg| arg == "-skip_frame"));
        assert_eq!(short.last().unwrap(), "C:\\tmp\\thumbs-1\\thumb-%02d.jpg");

        // Past 90 s, one keyframe seek per slice middle, one output each.
        let long = text(0.0, 192.0);
        let seeks: Vec<&str> = long
            .windows(2)
            .filter(|pair| pair[0] == "-ss")
            .map(|pair| pair[1].as_str())
            .collect();
        assert_eq!(seeks.len(), 24);
        assert_eq!((seeks[0], seeks[23]), ("4.000", "188.000"));
        assert_eq!(long.iter().filter(|arg| *arg == "-i").count(), 24);
        assert!(has_pair(&long, "-skip_frame", "nokey"));
        assert!(long.iter().any(|arg| arg == "-noaccurate_seek"));
        assert!(has_pair(&long, "-map", "23:V:0"));
        assert_eq!(long.last().unwrap(), "C:\\tmp\\thumbs-1\\thumb-24.jpg");

        // A zoomed range of 2.4 s deep in a long source starts there and is decoded in full.
        let range = text(1500.0, 2.4);
        assert!(has_pair(&range, "-ss", "1500.050"));
        assert!(has_pair(
            &range,
            "-vf",
            "tpad=stop_mode=clone:stop_duration=2.400,fps=1/0.100000:eof_action=pass,scale=-2:72"
        ));
        assert!(!range.iter().any(|arg| arg == "-skip_frame"));
    }

    #[test]
    fn a_thumbnail_range_is_cut_at_the_end_and_the_whole_source_is_no_range() {
        let range = |start: f64, end: f64| thumbnail_span(Some(TimeRange { start, end }), 60.0);
        assert_eq!(thumbnail_span(None, 60.0), Ok(None));
        assert_eq!(range(0.0, 60.0), Ok(None));
        assert_eq!(range(0.0, 75.0), Ok(None));
        assert_eq!(
            range(10.0, 12.5),
            Ok(Some(TimeRange {
                start: 10.0,
                end: 12.5
            }))
        );
        assert_eq!(
            range(50.0, 70.0),
            Ok(Some(TimeRange {
                start: 50.0,
                end: 60.0
            }))
        );
        for (start, end) in [
            (-1.0, 5.0),
            (60.0, 61.0),
            (5.0, 5.0),
            (6.0, 5.0),
            (f64::NAN, 5.0),
            (0.0, f64::INFINITY),
        ] {
            assert!(
                matches!(range(start, end), Err(PreviewError::InvalidRange { .. })),
                "{start}..{end} must be rejected"
            );
        }
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

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

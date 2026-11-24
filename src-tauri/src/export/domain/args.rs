//! The ffmpeg arguments for one output, ported from mgck's `buildSinglePassCommand`
//! (`single-pass-commands.ts`), `cropFilter` and `trimInput` (`ffmpeg-commands.ts`), which mgck in
//! turn ported from Gif Studio's `buildFilter` and `ffArgs`.

use std::ffi::OsString;
use std::path::Path;

use crate::export::domain::plan::{Crop, ExportPlan, seek_start};
use crate::export::domain::progress::{ENCODED, TAPPED, tap_share};
use crate::export::domain::settings::{Direction, FileType, GifDither, Rotation, Settings};

/// Where input seeking starts and how much of the input to read, in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputTrim {
    pub seek: f64,
    pub duration: f64,
}

/// Everything one run of ffmpeg needs, by reference.
#[derive(Debug, Clone, Copy)]
pub struct SinglePass<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
    pub file_type: FileType,
    pub settings: &'a Settings,
    pub crop: Option<Crop>,
    pub trim: Option<InputTrim>,
    /// Even output width, or `None` to keep the cropped width (made even).
    pub width: Option<u32>,
    /// Only honoured for MP4; [`ExportPlan::keep_sound`] has already checked that the source has
    /// sound and that the speed and direction let it keep up.
    pub keep_sound: bool,
    /// The caption image: a transparent PNG at the exact output size, or `None` for no captions.
    pub captions: Option<&'a Path>,
}

impl<'a> SinglePass<'a> {
    /// The run that makes `file_type` from `plan`, writing to `output`.
    pub fn from_plan(
        plan: &'a ExportPlan,
        file_type: FileType,
        output: &'a Path,
        captions: Option<&'a Path>,
    ) -> Self {
        let selection = &plan.selection;
        let trim = selection.trim.map(|trim| InputTrim {
            seek: seek_start(trim.start, selection.info.frame_rate),
            // Microseconds, ffmpeg's time unit, keep float noise off the command line.
            duration: ((trim.end - trim.start) * 1e6).round() / 1e6,
        });
        Self {
            input: &plan.source,
            output,
            file_type,
            settings: &selection.settings,
            crop: selection.crop,
            trim,
            width: Some(selection.size.width),
            keep_sound: plan.keep_sound,
            captions,
        }
    }
}

/// Options ahead of every run's own arguments: errors only on stderr, so its tail is the reason a
/// run failed; no stats line, since progress comes as frame times on stdout ([`job_args`]); and
/// `-n`, so a run never overwrites a file, whatever name it is given.
pub const RUNNER_ARGS: [&str; 6] = [
    "-hide_banner",
    "-loglevel",
    "error",
    "-nostats",
    "-n",
    "-nostdin",
];

/// What the app runs: [`RUNNER_ARGS`], then [`single_pass_args`] with progress reporting added
/// ([`crate::export::domain::progress`]). Each frame entering the encoder writes an
/// `encoded=<seconds>` line to stdout; an output that holds frames until the input ends also
/// splits its frames off before the holding filter into a null output, the tap, whose frames
/// write `tapped=<seconds>`. The tap only adds a consumer of frames the graph makes anyway, so the
/// output file is byte for byte what [`single_pass_args`] makes.
pub fn job_args(pass: &SinglePass<'_>) -> Vec<OsString> {
    let mut args: Vec<OsString> = RUNNER_ARGS.map(OsString::from).into();
    args.extend(pass_args(pass, true));
    args
}

/// One ffmpeg run per file type, in Gif Studio's filter order: crop, rotate, flips, speed, `fps`, a
/// lanczos scale, captions, then direction. Cropping and dropping frames first leaves later filters
/// as few pixels and frames as possible, and reversing last holds the smallest frames, since
/// `reverse` keeps every one of them in memory. This is mgck's command, without progress reporting:
/// the tests' reference for what [`job_args`] must produce.
#[cfg(test)]
pub fn single_pass_args(pass: &SinglePass<'_>) -> Vec<OsString> {
    pass_args(pass, false)
}

fn pass_args(pass: &SinglePass<'_>, report_progress: bool) -> Vec<OsString> {
    let settings = pass.settings;
    let mut filters: Vec<String> = rotate_filters(settings.rotation)
        .iter()
        .map(|f| (*f).to_owned())
        .collect();
    if settings.flip_horizontal {
        filters.push("hflip".to_owned());
    }
    if settings.flip_vertical {
        filters.push("vflip".to_owned());
    }
    if settings.speed != 1.0 {
        filters.push(format!("setpts=PTS/{}", settings.speed));
    }
    filters.push(format!("fps={}", settings.fps));
    let width = pass
        .width
        .map_or_else(|| "trunc(iw/2)*2".to_owned(), |w| w.to_string());
    filters.push(format!("scale={width}:-2:flags=lanczos"));

    let overlay = if pass.captions.is_some() {
        caption_overlay(pass.file_type)
    } else {
        String::new()
    };
    // Everything up to the direction filter: the frames of the selection, played once.
    let selection = format!(
        "[0:v]{}{}{}",
        crop_filter(pass.crop),
        filters.join(","),
        overlay
    );
    let direction = direction_filter(settings.direction);

    let mut args = trim_input(pass.input, pass.trim);
    if let Some(captions) = pass.captions {
        args.push("-i".into());
        args.push(captions.into());
    }

    // What follows the selection in the graph, ending in `[out]`, and the encoder's options.
    let (tail, options): (String, Vec<String>) = match pass.file_type {
        FileType::Gif => {
            // stats_mode=diff and diff_mode=rectangle spend the palette and the re-encoding on
            // what moves, which keeps static backgrounds clean.
            let tail = format!(
                "{direction},split[a][b];[a]palettegen=max_colors={}:stats_mode=diff[p];[b][p]paletteuse={}:diff_mode=rectangle[out]",
                settings.gif_colors,
                dither_option(settings.gif_dither)
            );
            (
                tail,
                vec!["-loop".into(), gif_loop(settings.plays).to_string()],
            )
        }
        FileType::Webp => {
            // libwebp_anim, which ffmpeg.wasm lacked, encodes the whole animation at once, so
            // frames store only what changed since the last: on footage with still regions files
            // came out up to 24 times smaller at the same quality, and on moving footage about
            // the same size per unit of quality as mgck's libwebp route
            // (docs/decisions/2026-11-24-webp-anim-encoder.md). `-loop` still reaches the file.
            let options = vec![
                "-c:v".into(),
                "libwebp_anim".into(),
                "-lossless".into(),
                "0".into(),
                "-quality".into(),
                settings.webp_quality.to_string(),
                "-compression_level".into(),
                "4".into(),
                "-loop".into(),
                settings.plays.to_string(),
            ];
            (format!("{direction}[out]"), options)
        }
        FileType::Mp4 => {
            let mut mp4 = vec![
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "medium".into(),
                "-crf".into(),
                "20".into(),
                "-movflags".into(),
                "+faststart".into(),
            ];
            if pass.keep_sound {
                // `?` keeps a source without sound from failing the whole run.
                mp4.extend(["-map", "0:a:0?", "-c:a", "aac", "-b:a", "160k"].map(str::to_owned));
                if settings.speed != 1.0 {
                    mp4.extend(["-af".to_owned(), format!("atempo={}", settings.speed)]);
                }
                mp4.push("-shortest".into());
            } else {
                mp4.push("-an".into());
            }
            (format!("{direction},format=yuv420p[out]"), mp4)
        }
    };

    let tap = report_progress && tap_share(pass.file_type, settings.direction).is_some();
    let graph = if tap {
        // Every tapped tail starts with a filter (a GIF's split or `reverse`), which now follows
        // the `[main]` label instead of a comma.
        let tail = tail
            .strip_prefix(',')
            .expect("a tapped output continues with a filter");
        format!("{selection},split[tap][main];[main]{tail}")
    } else {
        format!("{selection}{tail}")
    };
    args.extend(
        [
            "-filter_complex".to_owned(),
            graph,
            "-map".into(),
            "[out]".into(),
        ]
        .into_iter()
        .chain(options)
        .map(OsString::from),
    );
    if report_progress {
        args.extend(frame_times("v:0", ENCODED));
    }
    args.push(pass.output.into());
    if tap {
        args.extend(["-map", "[tap]"].map(OsString::from));
        args.extend(frame_times("v", TAPPED));
        args.extend(["-f", "null", "-"].map(OsString::from));
    }
    args
}

/// Output options that write `<key>=<seconds>` to stdout as each frame of the `stream` streams
/// enters its encoder. Per frame and flushed per line, unlike `-progress`, whose report follows
/// the muxed file and waits until every output has started.
fn frame_times(stream: &str, key: &str) -> [OsString; 4] {
    [
        format!("-stats_enc_pre:{stream}"),
        "pipe:1".to_owned(),
        format!("-stats_enc_pre_fmt:{stream}"),
        format!("{key}={{t}}"),
    ]
    .map(OsString::from)
}

/// Crop before scaling so later filters spend their work on the kept image. ffmpeg evaluates the
/// fractions against the frame it decodes, which is the displayed size since it auto-rotates
/// first; `max(1, …)` keeps a sliver crop from reaching zero pixels.
pub fn crop_filter(crop: Option<Crop>) -> String {
    match crop {
        Some(crop) => format!(
            "crop='max(1,iw*{})':'max(1,ih*{})':iw*{}:ih*{}:exact=1,",
            crop.width, crop.height, crop.x, crop.y
        ),
        None => String::new(),
    }
}

/// Seek before decoding (input `-ss`) so ffmpeg decodes from the keyframe before the selection
/// and encodes only the selection.
pub fn trim_input(input: &Path, trim: Option<InputTrim>) -> Vec<OsString> {
    let mut args = Vec::new();
    if let Some(trim) = trim {
        args.extend(
            [
                "-ss".to_owned(),
                trim.seek.to_string(),
                "-t".to_owned(),
                trim.duration.to_string(),
            ]
            .map(OsString::from),
        );
    }
    args.push("-i".into());
    args.push(input.into());
    args
}

/// Composites the caption image (input 1) after the scale, so it lands on output pixels the way
/// the preview draws it, and after the turn and flips, so it stays upright. A single image
/// repeats over every frame (overlay's default `eof_action=repeat`); centring it rather than
/// pinning it at 0:0 keeps a one-row rounding difference from shifting a caption off the edge. A
/// GIF composites in RGB, which its palette needs anyway, so the outline skips a 4:2:0 round trip;
/// MP4 and WebP encode 4:2:0 regardless.
fn caption_overlay(file_type: FileType) -> String {
    let format = if file_type == FileType::Gif {
        ":format=rgb"
    } else {
        ""
    };
    format!("[scaled];[scaled][1:v]overlay=(W-w)/2:(H-h)/2{format}")
}

/// `transpose=1` turns clockwise; a half turn is both flips.
fn rotate_filters(rotation: Rotation) -> &'static [&'static str] {
    match rotation {
        Rotation::None => &[],
        Rotation::Quarter => &["transpose=1"],
        Rotation::Half => &["hflip", "vflip"],
        Rotation::ThreeQuarters => &["transpose=2"],
    }
}

/// Appended to the video chain, which later steps continue with `,`. Boomerang plays the frames
/// forward, then a reversed copy, as one stream.
fn direction_filter(direction: Direction) -> &'static str {
    match direction {
        Direction::Forward => "",
        Direction::Reverse => ",reverse",
        Direction::Boomerang => ",split[f0][r0];[r0]reverse[r1];[f0][r1]concat=n=2:v=1:a=0",
    }
}

fn dither_option(dither: GifDither) -> &'static str {
    match dither {
        GifDither::Sierra24a => "dither=sierra2_4a",
        GifDither::FloydSteinberg => "dither=floyd_steinberg",
        GifDither::Bayer => "dither=bayer:bayer_scale=3",
        GifDither::None => "dither=none",
    }
}

/// The GIF muxer counts repeats, not plays: -1 plays once, 0 loops forever.
fn gif_loop(plays: u32) -> i64 {
    match plays {
        0 => 0,
        1 => -1,
        n => i64::from(n) - 1,
    }
}

#[cfg(test)]
mod tests;

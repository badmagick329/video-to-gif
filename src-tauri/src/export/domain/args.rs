//! The ffmpeg arguments for one output, ported from mgck's `buildSinglePassCommand`
//! (`single-pass-commands.ts`), `cropFilter` and `trimInput` (`ffmpeg-commands.ts`), which mgck in
//! turn ported from Gif Studio's `buildFilter` and `ffArgs`.

use std::ffi::OsString;
use std::path::Path;

use crate::export::domain::plan::{Crop, ExportPlan, seek_start};
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
/// run failed; progress as `key=value` lines on stdout instead of the stats line; and `-n`, so a
/// run never overwrites a file, whatever name it is given.
pub const RUNNER_ARGS: [&str; 8] = [
    "-hide_banner",
    "-loglevel",
    "error",
    "-nostats",
    "-progress",
    "pipe:1",
    "-n",
    "-nostdin",
];

/// [`RUNNER_ARGS`] followed by [`single_pass_args`].
pub fn job_args(pass: &SinglePass<'_>) -> Vec<OsString> {
    let mut args: Vec<OsString> = RUNNER_ARGS.map(OsString::from).into();
    args.extend(single_pass_args(pass));
    args
}

/// One ffmpeg run per file type, in Gif Studio's filter order: crop, rotate, flips, speed, `fps`, a
/// lanczos scale, captions, then direction. Cropping and dropping frames first leaves later filters
/// as few pixels and frames as possible, and reversing last holds the smallest frames, since
/// `reverse` keeps every one of them in memory.
pub fn single_pass_args(pass: &SinglePass<'_>) -> Vec<OsString> {
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
    let video = format!(
        "[0:v]{}{}{}{}",
        crop_filter(pass.crop),
        filters.join(","),
        overlay,
        direction_filter(settings.direction)
    );

    let mut args = trim_input(pass.input, pass.trim);
    if let Some(captions) = pass.captions {
        args.push("-i".into());
        args.push(captions.into());
    }

    let rest: Vec<String> = match pass.file_type {
        FileType::Gif => {
            // stats_mode=diff and diff_mode=rectangle spend the palette and the re-encoding on
            // what moves, which keeps static backgrounds clean.
            let graph = format!(
                "{video},split[a][b];[a]palettegen=max_colors={}:stats_mode=diff[p];[b][p]paletteuse={}:diff_mode=rectangle[out]",
                settings.gif_colors,
                dither_option(settings.gif_dither)
            );
            vec![
                "-filter_complex".into(),
                graph,
                "-map".into(),
                "[out]".into(),
                "-loop".into(),
                gif_loop(settings.plays).to_string(),
            ]
        }
        FileType::Webp => {
            // libwebp_anim, which ffmpeg.wasm lacked, encodes the whole animation at once, so
            // frames store only what changed since the last: on footage with still regions files
            // came out up to 24 times smaller at the same quality, and on moving footage about
            // the same size per unit of quality as mgck's libwebp route
            // (docs/decisions/2026-11-24-webp-anim-encoder.md). `-loop` still reaches the file.
            vec![
                "-filter_complex".into(),
                format!("{video}[out]"),
                "-map".into(),
                "[out]".into(),
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
            ]
        }
        FileType::Mp4 => {
            let mut mp4 = vec![
                "-filter_complex".into(),
                format!("{video},format=yuv420p[out]"),
                "-map".into(),
                "[out]".into(),
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
            mp4
        }
    };
    args.extend(rest.into_iter().map(OsString::from));
    args.push(pass.output.into());
    args
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

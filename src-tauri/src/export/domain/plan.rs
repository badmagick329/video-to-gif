//! What an export will do, checked: the [`ExportRequest`] the frontend sends, validated into an
//! [`ExportPlan`] whose numbers the argument builder and the runner trust. Also mgck's size and
//! naming helpers from `advanced.ts` (`croppedSourceSize`, `outputSize`, `evenDown`,
//! `outputSeconds`, `singlePassDownloadName`).

use std::path::PathBuf;

use serde::Deserialize;

use crate::export::domain::outcome::ExportError;
use crate::export::domain::settings::{
    Direction, FileType, Rotation, Settings, SettingsInput, Width, sound_drop_reason,
};
use crate::probe::domain::SourceInfo;

/// The selection, in seconds of the source (`ClipTrim`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Trim {
    pub start: f64,
    pub end: f64,
}

/// The kept rectangle as fractions of the source's displayed frame (`ClipCrop`). Fractions rather
/// than pixels so a box drawn over a proxy means the same box on the source.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// What Convert asks for (`ExportRequest` in `src/editor/lib/types.ts`). Untrusted until
/// [`ExportRequest::validate`] turns it into an [`ExportPlan`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    /// The source file.
    pub path: String,
    /// The source info `probe_source` returned for `path`, sent back and rechecked.
    pub info: SourceInfo,
    pub trim: Option<Trim>,
    pub crop: Option<Crop>,
    pub settings: SettingsInput,
    /// Keep the source's first sound track in the MP4, when the speed and direction allow.
    pub keep_sound: bool,
    /// The captions drawn as a transparent PNG at the exact output size, when there are any.
    pub captions: Option<Vec<u8>>,
}

/// What the memory estimate needs: everything that decides the output's frames, nothing about
/// where they go.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRequest {
    pub info: SourceInfo,
    pub trim: Option<Trim>,
    pub crop: Option<Crop>,
    pub settings: SettingsInput,
}

/// The frames an export makes: checked source info, selection, crop and settings, and the numbers
/// that follow from them.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub info: SourceInfo,
    /// `None` is the whole source.
    pub trim: Option<Trim>,
    /// `None` is the whole frame.
    pub crop: Option<Crop>,
    pub settings: Settings,
    /// The exact encoded size of every output.
    pub size: Size,
    /// Seconds of source the selection spans.
    pub selection_seconds: f64,
    /// Seconds each output plays for: speed changes the length and Boomerang doubles it.
    pub output_seconds: f64,
}

/// A checked export: the source, its [`Selection`], sound and captions, and the folder outputs go
/// to.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportPlan {
    pub source: PathBuf,
    pub selection: Selection,
    /// Whether the MP4 keeps sound: asked for, the source has some, and speed and direction let
    /// `atempo` keep it in step.
    pub keep_sound: bool,
    /// A PNG of exactly [`Selection::size`].
    pub captions: Option<Vec<u8>>,
    pub destination: PathBuf,
}

/// How far a trim's end may pass the probed duration and still count as the end: the player's
/// clock and ffprobe's duration can disagree by a rounding step.
const DURATION_SLACK: f64 = 0.001;
/// Slack for crop fractions summing past 1 through float rounding in the editor.
const CROP_SLACK: f64 = 1e-6;

impl ExportRequest {
    /// Checks everything the frontend sent and works out the output's numbers. `destination` is the
    /// folder the outputs go to. Paths must be absolute; whether they exist is the caller's I/O to
    /// check.
    pub fn validate(self, destination: PathBuf) -> Result<ExportPlan, ExportError> {
        let source = PathBuf::from(&self.path);
        if !source.is_absolute() {
            return Err(ExportError::NotFound { path: self.path });
        }
        if !destination.is_absolute() {
            return Err(ExportError::DestinationNotFound {
                path: destination.display().to_string(),
            });
        }
        let selection = Selection::validate(self.info, self.trim, self.crop, self.settings)?;
        if let Some(png) = &self.captions {
            check_caption_image(png, selection.size)?;
        }
        let settings = &selection.settings;
        let keep_sound = self.keep_sound
            && selection.info.has_audio()
            && sound_drop_reason(settings.speed, settings.direction).is_none();
        Ok(ExportPlan {
            source,
            selection,
            keep_sound,
            captions: self.captions,
            destination,
        })
    }
}

impl MemoryRequest {
    pub fn validate(self) -> Result<Selection, ExportError> {
        Selection::validate(self.info, self.trim, self.crop, self.settings)
    }
}

impl Selection {
    pub fn validate(
        info: SourceInfo,
        trim: Option<Trim>,
        crop: Option<Crop>,
        settings: SettingsInput,
    ) -> Result<Self, ExportError> {
        check_info(&info)?;
        let trim = trim
            .map(|trim| check_trim(trim, info.duration))
            .transpose()?;
        let crop = crop.map(check_crop).transpose()?;
        let settings = settings.clamp();
        if settings.file_types.is_empty() {
            return Err(invalid("tick at least one file type"));
        }
        let frame = Size {
            width: info.width,
            height: info.height,
        };
        let size = output_size(
            settings.width,
            cropped_source_size(crop, frame),
            settings.rotation,
        );
        let selection_seconds = trim.map_or(info.duration, |trim| trim.end - trim.start);
        let output_seconds = output_seconds(selection_seconds, settings.speed, settings.direction);
        Ok(Self {
            info,
            trim,
            crop,
            settings,
            size,
            selection_seconds,
            output_seconds,
        })
    }
}

fn invalid(reason: impl Into<String>) -> ExportError {
    ExportError::InvalidRequest {
        reason: reason.into(),
    }
}

/// Source info no probe could have produced; the frontend sends back what `probe_source`
/// returned, but the command cannot assume so.
fn check_info(info: &SourceInfo) -> Result<(), ExportError> {
    if !(info.duration.is_finite() && info.duration > 0.0) {
        Err(invalid("source duration must be positive"))
    } else if info.width == 0 || info.height == 0 {
        Err(invalid("source size must be positive"))
    } else if !(info.frame_rate.is_finite() && info.frame_rate > 0.0) {
        Err(invalid("source frame rate must be positive"))
    } else {
        Ok(())
    }
}

fn check_trim(trim: Trim, duration: f64) -> Result<Trim, ExportError> {
    let Trim { start, end } = trim;
    if !(start.is_finite() && end.is_finite()) || start < 0.0 || start >= end {
        return Err(invalid("the selection must start before it ends"));
    }
    if end > duration + DURATION_SLACK {
        return Err(invalid("the selection ends after the source does"));
    }
    Ok(Trim {
        start,
        end: end.min(duration),
    })
}

fn check_crop(crop: Crop) -> Result<Crop, ExportError> {
    let Crop {
        x,
        y,
        width,
        height,
    } = crop;
    let finite = [x, y, width, height].iter().all(|v| v.is_finite());
    if !finite
        || x < 0.0
        || y < 0.0
        || width <= 0.0
        || height <= 0.0
        || x + width > 1.0 + CROP_SLACK
        || y + height > 1.0 + CROP_SLACK
    {
        return Err(invalid("the crop must lie inside the frame"));
    }
    Ok(crop)
}

/// The caption image must be a PNG at exactly the output size: it is overlaid centred on every
/// frame, so another size would shift or clip the captions the preview showed. The PNG signature
/// and IHDR come first in every PNG, so the size is read from fixed offsets.
fn check_caption_image(png: &[u8], size: Size) -> Result<(), ExportError> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.len() < 24 || &png[..8] != SIGNATURE || &png[12..16] != b"IHDR" {
        return Err(invalid("the caption image is not a PNG"));
    }
    let width = u32::from_be_bytes(png[16..20].try_into().expect("four bytes"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("four bytes"));
    if (width, height) != (size.width, size.height) {
        return Err(invalid(format!(
            "the caption image is {width}×{height}, the output {}×{}",
            size.width, size.height
        )));
    }
    Ok(())
}

/// H.264 needs even sides; rounding down keeps the width within the crop.
pub fn even_down(value: u32) -> u32 {
    (value / 2 * 2).max(2)
}

/// Source pixels ffmpeg's crop filter keeps. It truncates `iw*fraction`, so this does too, which
/// keeps the editor's numbers equal to the file's.
pub fn cropped_source_size(crop: Option<Crop>, frame: Size) -> Size {
    let (width, height) = crop.map_or((1.0, 1.0), |crop| (crop.width, crop.height));
    Size {
        width: ((f64::from(frame.width) * width).floor() as u32).max(1),
        height: ((f64::from(frame.height) * height).floor() as u32).max(1),
    }
}

/// Exact encoded size. The width is even and never wider than the turned crop; the height
/// follows from `scale=W:-2`, which rounds to the nearest even number.
pub fn output_size(width: Width<u32>, cropped: Size, rotation: Rotation) -> Size {
    let turned = if rotation.is_sideways() {
        Size {
            width: cropped.height,
            height: cropped.width,
        }
    } else {
        cropped
    };
    let native = even_down(turned.width);
    let out_width = match width {
        Width::Original => native,
        Width::Pixels(pixels) => even_down(pixels).min(native),
    };
    let exact_height =
        f64::from(out_width) * f64::from(turned.height) / f64::from(turned.width) / 2.0;
    let height = ((exact_height.round() as u32) * 2).max(2);
    Size {
        width: out_width,
        height,
    }
}

/// Seconds of output: speed changes the length and Boomerang doubles it.
pub fn output_seconds(selection_seconds: f64, speed: f64, direction: Direction) -> f64 {
    let passes = if direction == Direction::Boomerang {
        2.0
    } else {
        1.0
    };
    selection_seconds / speed * passes
}

/// Where input seeking starts so the first frame out is the one the player showed at `start`.
///
/// The player at time t shows the frame whose span holds t, the last one starting at or before
/// t; input `-ss t` keeps frames starting at or after t, up to a frame later. Seeking one frame
/// earlier, plus a margin smaller than any frame, keeps exactly the frame covering t. The margin
/// (1 ms, or a quarter frame at over 250 fps) absorbs container timestamps rounded to the
/// millisecond. `frame_rate` is the probe's average, so a variable-rate source can still start a
/// frame off where frames bunch up. A trim at 0 stays at 0.
pub fn seek_start(start: f64, frame_rate: f64) -> f64 {
    let frame = 1.0 / frame_rate;
    let margin = (frame / 4.0).min(0.001);
    let seek = (start - frame + margin).max(0.0);
    // Microseconds are ffmpeg's own time unit; rounding keeps float noise off the command line.
    (seek * 1e6).round() / 1e6
}

/// The output's file name: the source's name with its extension swapped for the file type's
/// (mgck's `singlePassDownloadName`). A leading dot is part of the name, not an extension.
pub fn output_name(source_name: &str, file_type: FileType) -> String {
    let base = match source_name.rfind('.') {
        Some(dot) if dot > 0 => &source_name[..dot],
        _ => source_name,
    };
    format!("{base}.{}", file_type.extension())
}

/// `name` with ` (n)` before its extension, as browsers number a download whose name is taken;
/// `n == 0` is the name itself.
pub fn numbered_name(name: &str, n: u32) -> String {
    if n == 0 {
        return name.to_owned();
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{} ({n}){}", &name[..dot], &name[dot..]),
        _ => format!("{name} ({n})"),
    }
}

#[cfg(test)]
mod tests;

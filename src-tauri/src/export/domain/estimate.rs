//! Size estimates: how big each ticked file type will come out, worked out from sample encodes
//! that run the export's own arguments, so every setting (crop, speed, direction, captions)
//! reaches the samples.
//!
//! One method per file type, measured on 2026-11-24 against full exports of four clips (VTG-10):
//!
//! - **GIF**: three 1 s windows in one run, joined before `palettegen` so they share a palette,
//!   scaled by output frames over sampled frames. Median error 2.5%.
//! - **WebP**: six 0.5 s windows, a run each, read through the first-frame model
//!   ([`webp_estimate`]). Median error 3.4%. Three windows missed a heavy stretch by 35%, so
//!   coverage matters more than window length.
//! - **MP4**: the whole export. Sampling static footage was off by up to 275%, and libx264 is fast
//!   enough that the exact answer costs about what a sample would.
//!
//! A selection no longer than a file type's samples together is encoded whole, which is exact.

use serde::Serialize;

use crate::export::domain::args::InputTrim;
use crate::export::domain::plan::Selection;
use crate::export::domain::settings::{Direction, FileType};

/// Evenly spread windows of the selection: `count` of them, each `seconds` of output long.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Windows {
    pub count: usize,
    pub seconds: f64,
}

pub const GIF_WINDOWS: Windows = Windows {
    count: 3,
    seconds: 1.0,
};

pub const WEBP_WINDOWS: Windows = Windows {
    count: 6,
    seconds: 0.5,
};

/// How one file type's size is estimated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    /// Encode the selection as the export would and take the file's size.
    Whole,
    /// Encode windows of the selection and scale up.
    Sampled(Windows),
}

impl Method {
    pub fn for_selection(selection: &Selection, file_type: FileType) -> Self {
        let windows = match file_type {
            FileType::Gif => GIF_WINDOWS,
            FileType::Webp => WEBP_WINDOWS,
            FileType::Mp4 => return Method::Whole,
        };
        let sampled = windows.count as f64 * window_source_seconds(selection, windows);
        if selection.selection_seconds <= sampled {
            Method::Whole
        } else {
            Method::Sampled(windows)
        }
    }
}

/// Seconds of source one window reads: its output seconds at the chosen speed, so every window
/// yields the same number of frames whatever the speed.
fn window_source_seconds(selection: &Selection, windows: Windows) -> f64 {
    windows.seconds * selection.settings.speed
}

/// Where each window reads the source: window *i* of *k* is centred at
/// `start + (i + 0.5) × length / k`, kept inside the selection. Input seeking, as the export's
/// trim does, so each window decodes from its nearest keyframe rather than from the selection's
/// start.
pub fn sample_windows(selection: &Selection, windows: Windows) -> Vec<InputTrim> {
    let start = selection.trim.map_or(0.0, |trim| trim.start);
    let length = selection.selection_seconds;
    let duration = window_source_seconds(selection, windows).min(length);
    let micros = |seconds: f64| (seconds * 1e6).round() / 1e6;
    (0..windows.count)
        .map(|i| {
            let centre = start + (i as f64 + 0.5) * length / windows.count as f64;
            let seek = (centre - duration / 2.0).clamp(start, start + length - duration);
            InputTrim {
                seek: micros(seek),
                duration: micros(duration),
            }
        })
        .collect()
}

/// Frames the export will encode: the selection at the chosen speed, through `fps`, and twice for
/// Boomerang.
pub fn output_frames(selection: &Selection) -> u64 {
    let settings = &selection.settings;
    let once = (selection.selection_seconds / settings.speed * f64::from(settings.fps))
        .round()
        .max(1.0) as u64;
    match settings.direction {
        Direction::Boomerang => once * 2,
        Direction::Forward | Direction::Reverse => once,
    }
}

/// A GIF from its sample: the sample's bytes per frame times the output's frames. Each GIF frame
/// stands alone apart from a few hundred bytes of header and palette, so bytes grow with frames.
pub fn gif_estimate(sample_bytes: u64, sample_frames: u64, output_frames: u64) -> u64 {
    if sample_frames == 0 {
        return sample_bytes;
    }
    (sample_bytes as f64 * output_frames as f64 / sample_frames as f64).round() as u64
}

/// An animated WebP's bytes, split the way the first-frame model needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebpLayout {
    /// Everything outside the frames: RIFF header, `VP8X`, `ANIM` and any metadata.
    pub overhead: u64,
    /// The first `ANMF` chunk: a whole picture, unlike the frames after it.
    pub first: u64,
    /// The other `ANMF` chunks together.
    pub rest: u64,
}

/// Reads the chunk layout of a WebP file. A still WebP (no `ANMF`) counts as all first frame.
/// `None` when `bytes` is not a WebP.
pub fn webp_layout(bytes: &[u8]) -> Option<WebpLayout> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let mut frames: Vec<u64> = Vec::new();
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?) as usize;
        // Chunks are padded to an even length.
        let total = 8 + size + size % 2;
        if &bytes[at..at + 4] == b"ANMF" {
            frames.push(total as u64);
        }
        at += total;
    }
    let file = bytes.len() as u64;
    let Some((&first, rest)) = frames.split_first() else {
        return Some(WebpLayout {
            overhead: 0,
            first: file,
            rest: 0,
        });
    };
    let rest: u64 = rest.iter().sum();
    Some(WebpLayout {
        overhead: file - first - rest,
        first,
        rest,
    })
}

/// One WebP window's file, with the frames that entered its encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebpSample {
    pub layout: WebpLayout,
    /// Frame slots, counted as frames entered the encoder: libwebp_anim merges a frame that
    /// repeats the last into it, so `ANMF` chunks can be fewer than the frames shown.
    pub frames: u64,
}

/// The first-frame model: `overhead + first frame + (N − 1) × mean later frame`, each part the
/// median over the windows. libwebp_anim stores the first frame whole and later ones as changes,
/// so the first is several times the size of the rest and must not be scaled with them. A later
/// frame's mean spreads its window's later bytes over its later slots, so merged repeats count as
/// the nothing they cost.
pub fn webp_estimate(samples: &[WebpSample], output_frames: u64) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let overhead = median(samples.iter().map(|s| s.layout.overhead as f64));
    let first = median(samples.iter().map(|s| s.layout.first as f64));
    let later = median(
        samples
            .iter()
            .filter(|s| s.frames > 1)
            .map(|s| s.layout.rest as f64 / (s.frames - 1) as f64),
    );
    let later_frames = output_frames.saturating_sub(1) as f64;
    Some((overhead + first + later_frames * later).round() as u64)
}

/// The median, 0 for no values.
fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values: Vec<f64> = values.collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// One file type's estimate, sent over the estimate's channel as it settles and returned, in run
/// order, when every file type has.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SizeEstimate {
    Done {
        file_type: FileType,
        /// Expected bytes of the exported file.
        bytes: u64,
    },
    /// A sample run failed, or encoding the whole selection would hold more frames than fit in
    /// free memory. The UI shows no figure.
    Failed { file_type: FileType, reason: String },
    /// A newer change, playback or an export stopped it.
    Cancelled { file_type: FileType },
}

impl SizeEstimate {
    pub fn file_type(&self) -> FileType {
        match self {
            SizeEstimate::Done { file_type, .. }
            | SizeEstimate::Failed { file_type, .. }
            | SizeEstimate::Cancelled { file_type } => *file_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::export::domain::plan::MemoryRequest;

    fn selection(trim: Option<(f64, f64)>, patch: serde_json::Value) -> Selection {
        let mut settings = json!({
            "width": 480, "fps": 15, "speed": 1, "direction": "forward", "rotation": 0,
            "flipHorizontal": false, "flipVertical": false, "fileTypes": ["gif", "webp", "mp4"],
            "gifColors": 256, "gifDither": "sierra2_4a", "plays": 0, "webpQuality": 80
        });
        for (key, value) in patch.as_object().unwrap() {
            settings[key] = value.clone();
        }
        let request = json!({
            "info": {
                "duration": 60.0, "width": 1920, "height": 1080, "frameRate": 30.0,
                "videoCodec": "h264", "container": "mov,mp4,m4a,3gp,3g2,mj2", "audioCodec": null
            },
            "trim": trim.map(|(start, end)| json!({ "start": start, "end": end })),
            "settings": settings,
        });
        serde_json::from_value::<MemoryRequest>(request)
            .unwrap()
            .validate()
            .unwrap()
    }

    #[test]
    fn windows_are_centred_in_equal_slices_of_the_selection() {
        let windows = sample_windows(&selection(Some((10.0, 22.0)), json!({})), GIF_WINDOWS);
        assert_eq!(
            windows,
            vec![
                InputTrim {
                    seek: 11.5,
                    duration: 1.0
                },
                InputTrim {
                    seek: 15.5,
                    duration: 1.0
                },
                InputTrim {
                    seek: 19.5,
                    duration: 1.0
                },
            ]
        );
    }

    #[test]
    fn a_window_reads_more_source_at_higher_speed_and_stays_inside_the_selection() {
        let fast = selection(Some((0.0, 7.0)), json!({ "speed": 2 }));
        let windows = sample_windows(&fast, GIF_WINDOWS);
        assert!(windows.iter().all(|w| w.duration == 2.0));
        assert_eq!(windows[0].seek, 0.166667);
        assert!(windows.iter().all(|w| w.seek >= 0.0 && w.seek + 2.0 <= 7.0));
    }

    #[test]
    fn a_selection_no_longer_than_its_samples_is_encoded_whole() {
        let short = selection(Some((0.0, 3.0)), json!({}));
        assert_eq!(Method::for_selection(&short, FileType::Gif), Method::Whole);
        assert_eq!(Method::for_selection(&short, FileType::Webp), Method::Whole);
        let long = selection(Some((0.0, 3.1)), json!({}));
        assert_eq!(
            Method::for_selection(&long, FileType::Gif),
            Method::Sampled(GIF_WINDOWS)
        );
        // At 2× each window reads twice the source, so the samples cover 6 s.
        let fast = selection(Some((0.0, 5.0)), json!({ "speed": 2 }));
        assert_eq!(Method::for_selection(&fast, FileType::Gif), Method::Whole);
        // MP4 is always the whole export.
        let whole = selection(None, json!({}));
        assert_eq!(Method::for_selection(&whole, FileType::Mp4), Method::Whole);
    }

    #[test]
    fn output_frames_follow_speed_fps_and_boomerang() {
        assert_eq!(output_frames(&selection(Some((0.0, 10.0)), json!({}))), 150);
        assert_eq!(
            output_frames(&selection(Some((0.0, 10.0)), json!({ "speed": 0.5 }))),
            300
        );
        assert_eq!(
            output_frames(&selection(
                Some((0.0, 10.0)),
                json!({ "direction": "boomerang" })
            )),
            300
        );
    }

    #[test]
    fn a_gif_scales_with_its_frames() {
        assert_eq!(gif_estimate(1_000_000, 45, 450), 10_000_000);
        assert_eq!(gif_estimate(500, 0, 10), 500);
    }

    /// A WebP with `frames` ANMF chunks of the given payload sizes, after a VP8X and ANIM chunk.
    fn webp(frames: &[u32]) -> Vec<u8> {
        let mut body = b"WEBP".to_vec();
        let mut chunk = |name: &[u8], size: u32| {
            body.extend_from_slice(name);
            body.extend_from_slice(&size.to_le_bytes());
            body.extend(std::iter::repeat_n(0u8, (size + size % 2) as usize));
        };
        chunk(b"VP8X", 10);
        chunk(b"ANIM", 6);
        for &size in frames {
            chunk(b"ANMF", size);
        }
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend(body);
        file
    }

    #[test]
    fn a_webps_layout_separates_overhead_first_frame_and_the_rest() {
        let layout = webp_layout(&webp(&[1001, 200, 300])).unwrap();
        // RIFF header 12, VP8X 18, ANIM 14; the odd first frame is padded.
        assert_eq!(
            layout,
            WebpLayout {
                overhead: 44,
                first: 1010,
                rest: 516,
            }
        );
        assert_eq!(webp_layout(b"GIF89a"), None);
    }

    #[test]
    fn the_first_frame_model_scales_only_the_later_frames() {
        let sample = |first, rest, frames| WebpSample {
            layout: WebpLayout {
                overhead: 50,
                first,
                rest,
            },
            frames,
        };
        // Later frames cost 100 bytes a slot in each window (merged repeats included).
        let samples = [
            sample(5000, 700, 8),
            sample(7000, 700, 8),
            sample(6000, 700, 8),
        ];
        assert_eq!(webp_estimate(&samples, 100), Some(50 + 6000 + 99 * 100));
        assert_eq!(webp_estimate(&[], 100), None);
    }
}

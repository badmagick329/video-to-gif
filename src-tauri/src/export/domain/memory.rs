//! The frame-memory guard, ported from mgck's `heldCopies` and `frameMemoryLimitSeconds`
//! (`advanced.ts`). mgck checked against a fixed budget measured for ffmpeg.wasm's 2 GiB heap;
//! here the budget is the machine's free physical memory, read when the estimate is asked for.
//! Native ffmpeg does not stall at a heap limit, but a run that outgrows free RAM pages the whole
//! machine to disk for minutes, so the guard still refuses it up front.

use serde::Serialize;

use crate::export::domain::plan::{Selection, Size};
use crate::export::domain::settings::{Direction, FileType};

/// Bytes counted per pixel of a held frame: the GIF path's RGB with alpha, which overstates the
/// YUV that MP4 and WebP hold, on the safe side.
const BYTES_PER_PIXEL: u64 = 4;

/// Frame copies held at the peak, or 0 when frames stream through. A GIF queues every frame for
/// `paletteuse`, which waits for the palette made from the last one; `reverse` keeps another copy
/// until the input ends, and Boomerang adds its forward half to the GIF queue. File types run one
/// at a time, so the peak is the GIF run's whenever a GIF is ticked.
pub fn held_copies(direction: Direction, file_types: &[FileType]) -> u32 {
    let gif = file_types.contains(&FileType::Gif);
    match direction {
        Direction::Forward => u32::from(gif),
        _ if !gif => 1,
        Direction::Boomerang => 3,
        Direction::Reverse => 2,
    }
}

/// The longest selection, in seconds, these settings may convert within `budget` bytes, or `None`
/// when no frames are held (Forward without a GIF).
pub fn frame_memory_limit_seconds(
    size: Size,
    fps: u32,
    speed: f64,
    direction: Direction,
    file_types: &[FileType],
    budget: f64,
) -> Option<f64> {
    let copies = held_copies(direction, file_types);
    if copies == 0 {
        return None;
    }
    let frame_bytes = frame_bytes(size) as f64;
    let frames = budget / (frame_bytes * f64::from(copies));
    Some(frames / f64::from(fps) * speed)
}

/// Peak bytes of frames a selection's export holds: the inverse of
/// [`frame_memory_limit_seconds`], so `held_bytes <= budget` exactly when the selection is within
/// the limit.
pub fn held_bytes(selection: &Selection) -> u64 {
    let settings = &selection.settings;
    let copies = held_copies(settings.direction, &settings.file_types);
    // Frames of one pass through the selection; Boomerang's second pass is counted in `copies`.
    let frames = selection.selection_seconds / settings.speed * f64::from(settings.fps);
    (frames * frame_bytes(selection.size) as f64 * f64::from(copies)).ceil() as u64
}

fn frame_bytes(size: Size) -> u64 {
    u64::from(size.width) * u64::from(size.height) * BYTES_PER_PIXEL
}

/// The frame-memory estimate for a selection, returned by `estimate_export_memory` so the UI can
/// warn before Convert.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEstimate {
    /// Peak bytes of frames the export holds; 0 when frames stream through.
    pub held_bytes: u64,
    /// Free physical memory, in bytes, or `None` where the platform does not say.
    pub available_bytes: Option<u64>,
    /// The longest selection, in seconds, that fits in `available_bytes` with these settings;
    /// `None` when nothing is held or the free memory is unknown.
    pub limit_seconds: Option<f64>,
    /// Whether an export would start: always, unless the free memory is known and smaller than
    /// `held_bytes`.
    pub fits: bool,
}

impl MemoryEstimate {
    pub fn new(selection: &Selection, available_bytes: Option<u64>) -> Self {
        let held_bytes = held_bytes(selection);
        let settings = &selection.settings;
        let limit_seconds = available_bytes.and_then(|budget| {
            frame_memory_limit_seconds(
                selection.size,
                settings.fps,
                settings.speed,
                settings.direction,
                &settings.file_types,
                budget as f64,
            )
        });
        Self {
            held_bytes,
            available_bytes,
            limit_seconds,
            fits: available_bytes.is_none_or(|available| held_bytes <= available),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::domain::settings::FileType::*;

    const SIZE: Size = Size {
        width: 480,
        height: 270,
    };

    /// mgck's case: a minute of reversed GIF at 15 fps, two RGBA copies of each frame.
    const BUDGET: f64 = 480.0 * 270.0 * 4.0 * 2.0 * 15.0 * 60.0;

    fn limit(fps: u32, speed: f64, direction: Direction, types: &[FileType]) -> Option<f64> {
        frame_memory_limit_seconds(SIZE, fps, speed, direction, types, BUDGET)
    }

    fn close(actual: Option<f64>, expected: f64) -> bool {
        actual.is_some_and(|value| (value - expected).abs() < 1e-9)
    }

    #[test]
    fn the_limit_follows_frame_size_frame_rate_speed_and_held_copies() {
        use Direction::*;
        assert!(close(limit(15, 1.0, Reverse, &[Gif]), 60.0));
        // A forward GIF holds one copy, for the palette; forward MP4 and WebP stream their
        // frames and have no limit.
        assert!(close(limit(15, 1.0, Forward, &[Gif]), 120.0));
        assert_eq!(limit(15, 1.0, Forward, &[Webp, Mp4]), None);
        assert!(close(limit(30, 1.0, Reverse, &[Gif]), 30.0));
        // Half speed makes two output frames of each source frame.
        assert!(close(limit(15, 0.5, Reverse, &[Gif]), 30.0));
        // Boomerang's forward half queues for the palette too.
        assert!(close(limit(15, 1.0, Boomerang, &[Gif]), 40.0));
        // Without a GIF, nothing waits for a palette.
        assert!(close(limit(15, 1.0, Reverse, &[Webp, Mp4]), 120.0));
        assert!(close(limit(15, 1.0, Reverse, &[Gif, Mp4]), 60.0));
        let small = Size {
            width: 240,
            height: 136,
        };
        let longer = frame_memory_limit_seconds(small, 15, 1.0, Reverse, &[Gif], BUDGET).unwrap();
        assert!(longer > 3.9 * 60.0);
    }
}

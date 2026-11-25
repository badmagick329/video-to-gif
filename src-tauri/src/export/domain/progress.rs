//! How far an export's ffmpeg run has got, read from the frame times it writes to stdout.
//!
//! ffmpeg's own `-progress` report follows the output file, and only starts once every output has
//! begun: a GIF's `palettegen` and the `reverse` filter hold every frame until the input ends, and
//! libwebp_anim writes its one packet at the end, so their progress sat at 0 for most of the run.
//! Instead each run writes a line per frame as it enters the encoder (`-stats_enc_pre`), and an
//! output that holds frames back also feeds a copy of the frames, before the holding filter, to a
//! null output that writes a line per frame too: the tap, which moves while the input is read.

use crate::export::domain::plan::{Selection, output_seconds};
use crate::export::domain::settings::{Direction, FileType, Settings};

/// Key of the line ffmpeg writes as each frame enters the output's encoder; the value is the
/// frame's time in output seconds.
pub const ENCODED: &str = "encoded";

/// Key of the line ffmpeg writes as each frame reaches the tap; the value is the frame's time in
/// seconds of the selection played once at the chosen speed.
pub const TAPPED: &str = "tapped";

/// The smallest rise in progress worth reporting. Lines come once a frame, hundreds a second on a
/// fast encode, more than a progress bar can show.
const MIN_STEP: f64 = 0.005;

/// The share of the whole run that reading the input stands for when `file_type` with these
/// settings holds frames until the input ends, or `None` when frames flow to the encoder as they
/// are read, so the encoder's own times measure the run.
///
/// The shares come from timing each case on a 20 s 1080p clip at 480 and 1080 wide: reading the
/// input took 20 to 45% of an ffmpeg GIF's run, whose palette and dithering follow, and 3 to 31%
/// of a reversed WebP or MP4, where encoding dominates. Boomerang WebP and MP4 encode the forward
/// half as it is read, so they need no tap. A gifski GIF streams like them: ffmpeg's frames go
/// down the pipe as they are made, and the pipe holds back ffmpeg to gifski's pace.
pub fn tap_share(file_type: FileType, settings: &Settings) -> Option<f64> {
    if file_type == FileType::Gif && !settings.piped_to_gifski(file_type) {
        return Some(0.4);
    }
    (settings.direction == Direction::Reverse).then_some(0.2)
}

/// Turns one run's stdout lines into progress from 0 to 1 that never goes backward.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressMeter {
    /// The tap's share of the run and the seconds it will see, when the output holds frames.
    tap: Option<(f64, f64)>,
    output_seconds: f64,
    /// Fractions of each stream seen so far.
    tapped: f64,
    encoded: f64,
    reported: f64,
}

impl ProgressMeter {
    /// The meter for making `file_type` from `selection`.
    pub fn new(selection: &Selection, file_type: FileType) -> Self {
        let settings = &selection.settings;
        // The tap sits before the direction filter, so it sees the selection once.
        let tap_seconds = output_seconds(
            selection.selection_seconds,
            settings.speed,
            Direction::Forward,
        );
        Self {
            tap: tap_share(file_type, settings).map(|share| (share, tap_seconds)),
            output_seconds: selection.output_seconds,
            tapped: 0.0,
            encoded: 0.0,
            reported: 0.0,
        }
    }

    /// Reads one line of ffmpeg's stdout. Returns the run's progress when it has risen enough to
    /// be worth reporting; other lines, and values ffmpeg could not time (`N/A`), give `None`.
    pub fn read_line(&mut self, line: &str) -> Option<f64> {
        let (key, value) = line.trim().split_once('=')?;
        let seconds: f64 = value.parse().ok()?;
        match (key, self.tap) {
            (ENCODED, _) => self.encoded = advance(self.encoded, seconds, self.output_seconds),
            (TAPPED, Some((_, tap_seconds))) => {
                self.tapped = advance(self.tapped, seconds, tap_seconds);
            }
            _ => return None,
        }
        let progress = match self.tap {
            Some((share, _)) => share * self.tapped + (1.0 - share) * self.encoded,
            None => self.encoded,
        };
        (progress >= self.reported + MIN_STEP).then(|| {
            self.reported = progress;
            progress
        })
    }
}

/// `seconds` of `total` as a fraction, never below `seen` nor above 1.
fn advance(seen: f64, seconds: f64, total: f64) -> f64 {
    (seconds / total).clamp(seen, 1.0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::export::domain::plan::MemoryRequest;

    /// A 10 s selection of a 20 s source at `speed` and `direction`.
    fn selection(speed: f64, direction: &str) -> Selection {
        selection_by(speed, direction, "ffmpeg")
    }

    /// [`selection`], making its GIF with `encoder`.
    fn selection_by(speed: f64, direction: &str, encoder: &str) -> Selection {
        let request: MemoryRequest = serde_json::from_value(json!({
            "info": {
                "duration": 20.0, "width": 160, "height": 120, "frameRate": 10.0,
                "videoCodec": "h264", "container": "mov,mp4,m4a,3gp,3g2,mj2", "audioCodec": null
            },
            "trim": { "start": 5.0, "end": 15.0 },
            "settings": {
                "width": 80, "fps": 10, "speed": speed, "direction": direction, "rotation": 0,
                "flipHorizontal": false, "flipVertical": false, "fileTypes": ["gif"],
                "gifColors": 256, "gifDither": "sierra2_4a", "gifskiQuality": 90, "gifskiMotionQuality": 90, "plays": 0, "webpQuality": 80,
                "gifEncoder": encoder
            }
        }))
        .unwrap();
        request.validate().unwrap()
    }

    /// The progress each line reports, rounded clear of float noise.
    fn read(meter: &mut ProgressMeter, lines: &[&str]) -> Vec<Option<f64>> {
        lines
            .iter()
            .map(|line| meter.read_line(line).map(|p| (p * 1e9).round() / 1e9))
            .collect()
    }

    #[test]
    fn a_streaming_output_follows_the_encoders_frame_times() {
        let mut meter = ProgressMeter::new(&selection(1.0, "forward"), FileType::Webp);
        assert_eq!(
            read(
                &mut meter,
                &["encoded=0", "encoded=2.5", "encoded=10", "encoded=99"]
            ),
            [None, Some(0.25), Some(1.0), None]
        );
    }

    #[test]
    fn a_gif_counts_reading_the_input_then_encoding_it() {
        let mut meter = ProgressMeter::new(&selection(1.0, "forward"), FileType::Gif);
        // The palette holds every frame, so the tap runs to the end before anything is encoded.
        assert_eq!(
            read(
                &mut meter,
                &["tapped=5", "tapped=10", "encoded=5", "encoded=10"]
            ),
            [Some(0.2), Some(0.4), Some(0.7), Some(1.0)]
        );
    }

    #[test]
    fn reverse_taps_the_selection_once_and_boomerang_twice_as_long() {
        let mut reverse = ProgressMeter::new(&selection(2.0, "reverse"), FileType::Mp4);
        // At double speed the 10 s selection plays in 5 s, both at the tap and in the output.
        assert_eq!(
            read(&mut reverse, &["tapped=5", "encoded=2.5"]),
            [Some(0.2), Some(0.6)]
        );
        let mut boomerang = ProgressMeter::new(&selection(1.0, "boomerang"), FileType::Gif);
        assert_eq!(
            read(&mut boomerang, &["tapped=10", "encoded=10"]),
            [Some(0.4), Some(0.7)]
        );
        // Boomerang MP4 encodes its forward half as it is read: no tap.
        let mut streaming = ProgressMeter::new(&selection(1.0, "boomerang"), FileType::Mp4);
        assert_eq!(
            read(&mut streaming, &["tapped=10", "encoded=10"]),
            [None, Some(0.5)]
        );
    }

    #[test]
    fn a_gifski_gif_follows_ffmpegs_frame_times_unless_reversed() {
        let mut forward =
            ProgressMeter::new(&selection_by(1.0, "forward", "gifski"), FileType::Gif);
        assert_eq!(
            read(&mut forward, &["tapped=5", "encoded=5", "encoded=10"]),
            [None, Some(0.5), Some(1.0)]
        );
        let mut reverse =
            ProgressMeter::new(&selection_by(1.0, "reverse", "gifski"), FileType::Gif);
        assert_eq!(
            read(&mut reverse, &["tapped=10", "encoded=5"]),
            [Some(0.2), Some(0.6)]
        );
    }

    #[test]
    fn progress_never_goes_backward_nor_reports_steps_too_small_to_show() {
        let mut meter = ProgressMeter::new(&selection(1.0, "forward"), FileType::Mp4);
        assert_eq!(
            read(
                &mut meter,
                &[
                    "encoded=5",
                    "encoded=4",
                    "encoded=5.01",
                    "encoded=5.05",
                    "encoded=N/A",
                    "frame=3"
                ]
            ),
            [Some(0.5), None, None, Some(0.505), None, None]
        );
    }
}

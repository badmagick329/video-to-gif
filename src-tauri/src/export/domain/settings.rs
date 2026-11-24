//! Advanced mode's output settings, ported from mgck's `AdvancedSettings` (`advanced.ts`).
//!
//! [`SettingsInput`] is the shape the frontend sends; [`Settings`] is the same choices after
//! [`SettingsInput::clamp`] has pulled every number into mgck's `settingLimits`, which is the only
//! way the rest of the slice gets one.

use std::fmt;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize};

/// An encoded output. Called "file type" because mgck's "output format" named a simple-mode
/// preset. Declaration order is the order a run makes them in: GIF, WebP, MP4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Gif,
    Webp,
    Mp4,
}

impl FileType {
    /// The file name extension, without the dot; ffmpeg picks the muxer from it.
    pub fn extension(self) -> &'static str {
        match self {
            FileType::Gif => "gif",
            FileType::Webp => "webp",
            FileType::Mp4 => "mp4",
        }
    }
}

/// How the selection plays: as recorded, backward, or forward then backward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Forward,
    Reverse,
    Boomerang,
}

/// `paletteuse`'s dithering, by ffmpeg's own names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum GifDither {
    #[serde(rename = "sierra2_4a")]
    Sierra24a,
    #[serde(rename = "floyd_steinberg")]
    FloydSteinberg,
    #[serde(rename = "bayer")]
    Bayer,
    #[serde(rename = "none")]
    None,
}

/// Clockwise quarter turns, sent as degrees (0, 90, 180 or 270).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "u16")]
pub enum Rotation {
    None,
    Quarter,
    Half,
    ThreeQuarters,
}

impl Rotation {
    /// A quarter turn swaps the output's width and height.
    pub fn is_sideways(self) -> bool {
        matches!(self, Rotation::Quarter | Rotation::ThreeQuarters)
    }
}

impl TryFrom<u16> for Rotation {
    type Error = String;

    fn try_from(degrees: u16) -> Result<Self, Self::Error> {
        match degrees {
            0 => Ok(Rotation::None),
            90 => Ok(Rotation::Quarter),
            180 => Ok(Rotation::Half),
            270 => Ok(Rotation::ThreeQuarters),
            other => Err(format!("rotation must be 0, 90, 180 or 270, not {other}")),
        }
    }
}

/// The output width the user asked for, before capping at the crop: a number of pixels, or
/// `"original"` for the crop's own width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Width<T> {
    Pixels(T),
    Original,
}

impl<'de> Deserialize<'de> for Width<f64> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct WidthVisitor;
        impl Visitor<'_> for WidthVisitor {
            type Value = Width<f64>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(r#"a number of pixels or "original""#)
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(Width::Pixels(value))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Width::Pixels(value as f64))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Width::Pixels(value as f64))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "original" => Ok(Width::Original),
                    other => Err(E::invalid_value(de::Unexpected::Str(other), &self)),
                }
            }
        }
        deserializer.deserialize_any(WidthVisitor)
    }
}

/// A number setting's inclusive range, mgck's `settingLimits`.
#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub min: u32,
    pub max: u32,
}

impl Limit {
    /// mgck's `clampSetting`: rounds, then clamps. A non-finite value (which JSON cannot carry,
    /// but the type allows) falls to the minimum.
    pub fn clamp(self, value: f64) -> u32 {
        if !value.is_finite() {
            return self.min;
        }
        value
            .round()
            .clamp(f64::from(self.min), f64::from(self.max)) as u32
    }
}

pub const WIDTH_LIMIT: Limit = Limit { min: 16, max: 4096 };
pub const FPS_LIMIT: Limit = Limit { min: 1, max: 60 };
pub const GIF_COLORS_LIMIT: Limit = Limit { min: 2, max: 256 };
pub const WEBP_QUALITY_LIMIT: Limit = Limit { min: 1, max: 100 };
/// mgck offers 0 (forever) to 3 plays; 65535 is the most the GIF and WebP loop fields hold.
pub const PLAYS_LIMIT: Limit = Limit { min: 0, max: 65535 };

/// mgck's speed slider runs from 0.25× to 3× (`speedSteps`). Any speed between works, so a value
/// off the steps is clamped rather than refused.
pub const SPEED_RANGE: (f64, f64) = (0.25, 3.0);

/// `atempo` keeps sound in step only between these speeds.
pub const SOUND_SPEED_RANGE: (f64, f64) = (0.5, 2.0);

/// The settings as the frontend sends them (`AdvancedSettings` in `src/editor/lib/advanced.ts`).
///
/// The caption text and size are not read: captions arrive already drawn, as a PNG
/// (`ExportRequest.captions`), so serde ignores those fields.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub width: Width<f64>,
    pub fps: f64,
    pub speed: f64,
    pub direction: Direction,
    pub rotation: Rotation,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub file_types: Vec<FileType>,
    pub gif_colors: f64,
    pub gif_dither: GifDither,
    pub plays: f64,
    pub webp_quality: f64,
}

/// Settings within mgck's limits, with file types de-duplicated into the order a run makes them.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub width: Width<u32>,
    pub fps: u32,
    pub speed: f64,
    pub direction: Direction,
    pub rotation: Rotation,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    /// In [`FileType`] order, each at most once.
    pub file_types: Vec<FileType>,
    pub gif_colors: u32,
    pub gif_dither: GifDither,
    /// Times a GIF or WebP plays; 0 is forever. MP4 players decide looping themselves.
    pub plays: u32,
    pub webp_quality: u32,
}

impl SettingsInput {
    /// Pulls every number into its limit, as mgck's form does on input. File types are sorted and
    /// de-duplicated; whether any are left is the request's check.
    pub fn clamp(self) -> Settings {
        let mut file_types = self.file_types;
        file_types.sort();
        file_types.dedup();
        let speed = if self.speed.is_finite() {
            self.speed.clamp(SPEED_RANGE.0, SPEED_RANGE.1)
        } else {
            1.0
        };
        Settings {
            width: match self.width {
                Width::Pixels(pixels) => Width::Pixels(WIDTH_LIMIT.clamp(pixels)),
                Width::Original => Width::Original,
            },
            fps: FPS_LIMIT.clamp(self.fps),
            speed,
            direction: self.direction,
            rotation: self.rotation,
            flip_horizontal: self.flip_horizontal,
            flip_vertical: self.flip_vertical,
            file_types,
            gif_colors: GIF_COLORS_LIMIT.clamp(self.gif_colors),
            gif_dither: self.gif_dither,
            plays: PLAYS_LIMIT.clamp(self.plays),
            webp_quality: WEBP_QUALITY_LIMIT.clamp(self.webp_quality),
        }
    }
}

/// Why MP4 sound can't follow these settings, or `None` when it can: `atempo` keeps it in step
/// only between 0.5× and 2×, and nothing plays sound backward. mgck's `soundDropReason`, worded
/// the same so the UI and a log read alike.
pub fn sound_drop_reason(speed: f64, direction: Direction) -> Option<String> {
    match direction {
        Direction::Reverse => return Some("No sound in Reverse.".to_owned()),
        Direction::Boomerang => return Some("No sound in Boomerang.".to_owned()),
        Direction::Forward => {}
    }
    let (min, max) = SOUND_SPEED_RANGE;
    (speed < min || speed > max)
        .then(|| format!("No sound at {speed}×; it keeps up from {min}× to {max}×."))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// mgck's `defaultAdvancedSettings`, as the frontend serializes it.
    fn defaults() -> serde_json::Value {
        json!({
            "width": 480,
            "fps": 15,
            "speed": 1,
            "direction": "forward",
            "rotation": 0,
            "flipHorizontal": false,
            "flipVertical": false,
            "topText": "",
            "bottomText": "",
            "captionSize": 12,
            "fileTypes": ["gif"],
            "gifColors": 256,
            "gifDither": "sierra2_4a",
            "plays": 0,
            "webpQuality": 80,
            "keepSound": false
        })
    }

    fn parse(patch: serde_json::Value) -> Settings {
        let mut value = defaults();
        for (key, field) in patch.as_object().unwrap() {
            value[key] = field.clone();
        }
        serde_json::from_value::<SettingsInput>(value)
            .unwrap()
            .clamp()
    }

    #[test]
    fn defaults_arrive_as_sent() {
        let settings = parse(json!({}));
        assert_eq!(
            settings,
            Settings {
                width: Width::Pixels(480),
                fps: 15,
                speed: 1.0,
                direction: Direction::Forward,
                rotation: Rotation::None,
                flip_horizontal: false,
                flip_vertical: false,
                file_types: vec![FileType::Gif],
                gif_colors: 256,
                gif_dither: GifDither::Sierra24a,
                plays: 0,
                webp_quality: 80,
            }
        );
    }

    #[test]
    fn numbers_are_rounded_and_clamped_to_mgcks_limits() {
        let low = parse(json!({
            "width": 3, "fps": 0, "gifColors": 1, "webpQuality": -5, "plays": -1, "speed": 0.1
        }));
        assert_eq!(low.width, Width::Pixels(16));
        assert_eq!(low.fps, 1);
        assert_eq!(low.gif_colors, 2);
        assert_eq!(low.webp_quality, 1);
        assert_eq!(low.plays, 0);
        assert_eq!(low.speed, 0.25);

        let high = parse(json!({
            "width": 9000, "fps": 240, "gifColors": 1000, "webpQuality": 101, "speed": 10
        }));
        assert_eq!(high.width, Width::Pixels(4096));
        assert_eq!(high.fps, 60);
        assert_eq!(high.gif_colors, 256);
        assert_eq!(high.webp_quality, 100);
        assert_eq!(high.speed, 3.0);

        assert_eq!(parse(json!({ "fps": 14.6 })).fps, 15);
        assert_eq!(parse(json!({ "width": "original" })).width, Width::Original);
    }

    #[test]
    fn file_types_come_out_in_run_order_once_each() {
        let settings = parse(json!({ "fileTypes": ["mp4", "gif", "mp4", "webp"] }));
        assert_eq!(
            settings.file_types,
            vec![FileType::Gif, FileType::Webp, FileType::Mp4]
        );
    }

    #[test]
    fn impossible_choices_do_not_parse() {
        let mut value = defaults();
        value["rotation"] = json!(45);
        assert!(serde_json::from_value::<SettingsInput>(value).is_err());
        let mut value = defaults();
        value["width"] = json!("wide");
        assert!(serde_json::from_value::<SettingsInput>(value).is_err());
        let mut value = defaults();
        value["fileTypes"] = json!(["apng"]);
        assert!(serde_json::from_value::<SettingsInput>(value).is_err());
    }

    #[test]
    fn the_sound_note_says_why_sound_is_dropped() {
        use Direction::*;
        assert_eq!(sound_drop_reason(1.0, Forward), None);
        assert_eq!(sound_drop_reason(0.5, Forward), None);
        assert_eq!(sound_drop_reason(2.0, Forward), None);
        assert_eq!(
            sound_drop_reason(3.0, Forward).as_deref(),
            Some("No sound at 3×; it keeps up from 0.5× to 2×.")
        );
        assert_eq!(
            sound_drop_reason(1.0, Reverse).as_deref(),
            Some("No sound in Reverse.")
        );
        assert_eq!(
            sound_drop_reason(1.0, Boomerang).as_deref(),
            Some("No sound in Boomerang.")
        );
    }
}

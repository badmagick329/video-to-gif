//! Request validation and mgck's size, length and naming numbers.

use serde_json::{Value, json};

use super::*;
use crate::export::domain::args::{InputTrim, SinglePass};
use crate::export::domain::memory::MemoryEstimate;

fn info() -> Value {
    json!({
        "duration": 10.0,
        "width": 1920,
        "height": 1080,
        "frameRate": 25.0,
        "videoCodec": "h264",
        "container": "mov,mp4,m4a,3gp,3g2,mj2",
        "audioCodec": "aac"
    })
}

/// A request as Convert sends it, with mgck's default settings.
fn request() -> Value {
    json!({
        "path": "C:\\clips\\dance.mp4",
        "info": info(),
        "settings": {
            "width": 480, "fps": 15, "speed": 1, "direction": "forward", "rotation": 0,
            "flipHorizontal": false, "flipVertical": false, "topText": "", "bottomText": "",
            "captionSize": 12, "fileTypes": ["gif"], "gifColors": 256,
            "gifDither": "sierra2_4a", "gifEncoder": "ffmpeg", "gifskiQuality": 90, "gifskiMotionQuality": 90, "plays": 0, "webpQuality": 80, "keepSound": false
        },
        "keepSound": false
    })
}

fn with(patch: Value) -> Value {
    let mut value = request();
    merge(&mut value, patch);
    value
}

fn merge(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch,
    }
}

fn destination() -> PathBuf {
    PathBuf::from("C:\\out")
}

fn validate(value: Value) -> Result<ExportPlan, ExportError> {
    serde_json::from_value::<ExportRequest>(value)
        .unwrap()
        .validate(destination())
}

fn plan(patch: Value) -> ExportPlan {
    validate(with(patch)).unwrap()
}

fn rejected(patch: Value) -> String {
    match validate(with(patch)) {
        Err(ExportError::InvalidRequest { reason }) => reason,
        other => panic!("expected an invalid request, got {other:?}"),
    }
}

/// The smallest PNG header that reads as `width`×`height`.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes
}

#[test]
fn the_frontends_request_parses_and_sizes_the_output() {
    let plan = plan(json!({}));
    assert_eq!(plan.source, PathBuf::from("C:\\clips\\dance.mp4"));
    assert_eq!(plan.destination, destination());
    assert_eq!(
        plan.selection.size,
        Size {
            width: 480,
            height: 270
        }
    );
    assert_eq!(plan.selection.selection_seconds, 10.0);
    assert_eq!(plan.selection.output_seconds, 10.0);
    assert_eq!(plan.selection.trim, None);
    assert_eq!(plan.captions, None);
}

#[test]
fn relative_paths_are_refused() {
    assert!(matches!(
        validate(with(json!({ "path": "dance.mp4" }))),
        Err(ExportError::NotFound { .. })
    ));
    let request: ExportRequest = serde_json::from_value(request()).unwrap();
    assert!(matches!(
        request.validate(PathBuf::from("out")),
        Err(ExportError::DestinationNotFound { .. })
    ));
}

#[test]
fn source_info_no_probe_could_produce_is_refused() {
    rejected(json!({ "info": { "duration": 0.0 } }));
    rejected(json!({ "info": { "width": 0 } }));
    rejected(json!({ "info": { "frameRate": 0.0 } }));
}

#[test]
fn the_selection_must_lie_inside_the_source() {
    rejected(json!({ "trim": { "start": 3.0, "end": 3.0 } }));
    rejected(json!({ "trim": { "start": -1.0, "end": 3.0 } }));
    rejected(json!({ "trim": { "start": 3.0, "end": 10.5 } }));
    // A rounding step past the end is the end.
    let plan = plan(json!({ "trim": { "start": 1.5, "end": 10.0005 } }));
    assert_eq!(
        plan.selection.trim,
        Some(Trim {
            start: 1.5,
            end: 10.0
        })
    );
    assert_eq!(plan.selection.selection_seconds, 8.5);
}

#[test]
fn the_crop_must_lie_inside_the_frame() {
    rejected(json!({ "crop": { "x": 0.6, "y": 0.0, "width": 0.5, "height": 1.0 } }));
    rejected(json!({ "crop": { "x": 0.0, "y": -0.1, "width": 0.5, "height": 0.5 } }));
    rejected(json!({ "crop": { "x": 0.0, "y": 0.0, "width": 0.0, "height": 0.5 } }));
    let plan = plan(json!({ "crop": { "x": 0.25, "y": 0.0, "width": 0.75, "height": 1.0 } }));
    assert_eq!(plan.selection.size.width, 480);
}

#[test]
fn an_export_needs_a_file_type() {
    assert_eq!(
        rejected(json!({ "settings": { "fileTypes": [] } })),
        "tick at least one file type"
    );
}

#[test]
fn the_caption_image_must_be_a_png_at_the_output_size() {
    let plan = plan(json!({ "captions": png(480, 270) }));
    assert_eq!(plan.captions, Some(png(480, 270)));
    assert!(rejected(json!({ "captions": png(480, 272) })).contains("480×272"));
    assert_eq!(
        rejected(json!({ "captions": [1, 2, 3] })),
        "the caption image is not a PNG"
    );
}

#[test]
fn mp4_drops_sound_when_the_speed_or_direction_cannot_keep_it() {
    let sound =
        |settings: Value| plan(json!({ "keepSound": true, "settings": settings })).keep_sound;
    assert!(sound(json!({})));
    assert!(sound(json!({ "speed": 0.5 })));
    assert!(sound(json!({ "speed": 2 })));
    for settings in [
        json!({ "speed": 0.25 }),
        json!({ "speed": 0.33 }),
        json!({ "speed": 2.5 }),
        json!({ "speed": 3 }),
        json!({ "direction": "reverse" }),
        json!({ "direction": "boomerang" }),
        json!({ "direction": "reverse", "speed": 1.5 }),
    ] {
        assert!(!sound(settings.clone()), "{settings}");
    }
    // Nothing to keep from a silent source, and nothing unless asked.
    assert!(!plan(json!({ "keepSound": true, "info": { "audioCodec": null } })).keep_sound);
    assert!(!plan(json!({})).keep_sound);
}

#[test]
fn speed_changes_the_length_and_boomerang_doubles_it() {
    use Direction::*;
    assert_eq!(output_seconds(4.0, 1.0, Forward), 4.0);
    assert_eq!(output_seconds(4.0, 2.0, Forward), 2.0);
    assert_eq!(output_seconds(4.0, 2.0, Boomerang), 4.0);
    assert_eq!(output_seconds(4.0, 0.5, Reverse), 8.0);
    let plan = plan(json!({
        "trim": { "start": 1.0, "end": 5.0 },
        "settings": { "speed": 2, "direction": "boomerang" }
    }));
    assert_eq!(plan.selection.output_seconds, 4.0);
}

#[test]
fn quarter_turns_swap_the_output_size_and_a_half_turn_does_not() {
    let portrait = Size {
        width: 1080,
        height: 1920,
    };
    let size = |width, rotation| output_size(width, portrait, rotation);
    assert_eq!(
        size(Width::Pixels(480), Rotation::None),
        Size {
            width: 480,
            height: 854
        }
    );
    assert_eq!(
        size(Width::Pixels(480), Rotation::Quarter),
        Size {
            width: 480,
            height: 270
        }
    );
    assert_eq!(
        size(Width::Original, Rotation::ThreeQuarters),
        Size {
            width: 1920,
            height: 1080
        }
    );
    assert_eq!(
        size(Width::Pixels(480), Rotation::Half),
        size(Width::Pixels(480), Rotation::None)
    );
}

#[test]
fn width_is_even_capped_at_the_crop_and_the_height_follows_scale_minus_2() {
    let frame = Size {
        width: 1920,
        height: 1080,
    };
    let full = cropped_source_size(None, frame);
    let size = |width, cropped| output_size(width, cropped, Rotation::None);
    assert_eq!(
        size(Width::Pixels(480), full),
        Size {
            width: 480,
            height: 270
        }
    );
    assert_eq!(size(Width::Pixels(481), full).width, 480);
    assert_eq!(
        size(Width::Original, full),
        Size {
            width: 1920,
            height: 1080
        }
    );
    // A 301-pixel crop truncates like ffmpeg's crop filter and caps at 300.
    let narrow = cropped_source_size(
        Some(Crop {
            x: 0.0,
            y: 0.0,
            width: 301.0 / 1920.0,
            height: 0.5,
        }),
        frame,
    );
    assert_eq!(
        narrow,
        Size {
            width: 301,
            height: 540
        }
    );
    assert_eq!(
        size(Width::Pixels(640), narrow),
        Size {
            width: 300,
            height: 538
        }
    );
    assert_eq!(even_down(1), 2);
}

#[test]
fn crop_pixels_come_from_the_displayed_size() {
    // A phone video stored 1920×1080 with a quarter-turn display matrix probes as 1080×1920.
    let plan = plan(json!({
        "info": { "width": 1080, "height": 1920 },
        "crop": { "x": 0.0, "y": 0.25, "width": 1.0, "height": 0.5 },
        "settings": { "width": "original" }
    }));
    assert_eq!(
        plan.selection.size,
        Size {
            width: 1080,
            height: 960
        }
    );
}

#[test]
fn output_names_swap_the_extension_for_the_file_type() {
    assert_eq!(
        output_name("dance.clip.mov", FileType::Gif),
        "dance.clip.gif"
    );
    assert_eq!(output_name("dance", FileType::Webp), "dance.webp");
    assert_eq!(output_name("dance.mp4", FileType::Mp4), "dance.mp4");
    assert_eq!(output_name(".hidden", FileType::Gif), ".hidden.gif");
}

#[test]
fn taken_names_are_numbered_before_the_extension() {
    assert_eq!(numbered_name("dance.gif", 0), "dance.gif");
    assert_eq!(numbered_name("dance.gif", 1), "dance (1).gif");
    assert_eq!(numbered_name("dance.clip.gif", 12), "dance.clip (12).gif");
    assert_eq!(numbered_name("dance", 2), "dance (2)");
}

#[test]
fn seeking_starts_a_frame_early_to_land_on_the_frame_the_player_showed() {
    // At 10 fps the player at 0.35 s shows frame 3 (0.3 s); `-ss 0.35` would start at frame 4.
    assert_eq!(seek_start(0.35, 10.0), 0.251);
    // On a frame's own start the player shows that frame; seeking just past the previous one
    // keeps it.
    assert_eq!(seek_start(0.3, 10.0), 0.201);
    assert_eq!(seek_start(0.0, 10.0), 0.0);
    assert_eq!(seek_start(0.05, 10.0), 0.0);
    // Past 250 fps the margin shrinks to a quarter frame, so it never reaches the frame before.
    assert_eq!(seek_start(1.0, 1000.0), 0.99925);
}

#[test]
fn the_run_reads_the_plan_through_the_ported_builder() {
    let plan = plan(json!({
        "keepSound": true,
        "trim": { "start": 1.5, "end": 4.0 },
        "crop": { "x": 0.25, "y": 0.1, "width": 0.5, "height": 0.8 },
        "settings": { "fileTypes": ["mp4"] }
    }));
    let output = PathBuf::from("C:\\out\\dance.part-1-1.mp4");
    let pass = SinglePass::from_plan(&plan, FileType::Mp4, &output, None);
    assert_eq!(
        pass.trim,
        Some(InputTrim {
            seek: 1.461,
            duration: 2.5
        })
    );
    assert_eq!(pass.width, Some(480));
    assert!(pass.keep_sound);
    assert_eq!(pass.input, plan.source.as_path());
}

#[test]
fn the_memory_estimate_refuses_what_free_memory_cannot_hold() {
    let request: MemoryRequest = serde_json::from_value(with(json!({
        "settings": { "direction": "reverse" },
        "trim": { "start": 0.0, "end": 4.0 }
    })))
    .unwrap();
    let selection = request.validate().unwrap();
    // 4 s of 480×270 at 15 fps, two RGBA copies.
    let held = 60 * 480 * 270 * 4 * 2;
    let estimate = MemoryEstimate::new(&selection, Some(held));
    assert_eq!(estimate.held_bytes, held);
    assert!(estimate.fits);
    assert!((estimate.limit_seconds.unwrap() - 4.0).abs() < 1e-9);
    assert!(!MemoryEstimate::new(&selection, Some(held - 1)).fits);
    // Unknown free memory never blocks a run.
    let unknown = MemoryEstimate::new(&selection, None);
    assert!(unknown.fits);
    assert_eq!(unknown.limit_seconds, None);
}

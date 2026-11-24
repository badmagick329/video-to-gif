//! mgck's `discordgifs-advanced-commands.test.ts` command cases, ported. WebP now encodes with
//! libwebp_anim; everything else expects mgck's exact arguments.

use std::path::Path;

use super::*;
use crate::export::domain::settings::Width;

fn defaults() -> Settings {
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
}

/// Options beyond the settings, as mgck's test `extra` patch.
#[derive(Clone, Copy)]
struct Extra {
    crop: Option<Crop>,
    trim: Option<InputTrim>,
    width: Option<u32>,
    keep_sound: bool,
    captions: bool,
}

const EXTRA: Extra = Extra {
    crop: None,
    trim: None,
    width: Some(480),
    keep_sound: false,
    captions: false,
};

fn command_with(
    file_type: FileType,
    patch: impl FnOnce(&mut Settings),
    extra: Extra,
) -> Vec<String> {
    build(file_type, patch, extra, single_pass_args)
}

fn build(
    file_type: FileType,
    patch: impl FnOnce(&mut Settings),
    extra: Extra,
    args: fn(&SinglePass<'_>) -> Vec<OsString>,
) -> Vec<String> {
    let mut settings = defaults();
    patch(&mut settings);
    let output = format!("output.{}", file_type.extension());
    let pass = SinglePass {
        input: Path::new("input.mp4"),
        output: Path::new(&output),
        file_type,
        settings: &settings,
        crop: extra.crop,
        trim: extra.trim,
        width: extra.width,
        keep_sound: extra.keep_sound,
        captions: extra.captions.then_some(Path::new("captions.png")),
    };
    args(&pass)
        .into_iter()
        .map(|arg| arg.into_string().unwrap())
        .collect()
}

fn command(file_type: FileType, patch: impl FnOnce(&mut Settings)) -> Vec<String> {
    command_with(file_type, patch, EXTRA)
}

fn plain(file_type: FileType) -> Vec<String> {
    command(file_type, |_| {})
}

fn graph(args: &[String]) -> &str {
    let at = args.iter().position(|a| a == "-filter_complex").unwrap();
    &args[at + 1]
}

/// The video chain without its `[0:v]` input label.
fn chain(args: &[String]) -> &str {
    graph(args).strip_prefix("[0:v]").unwrap()
}

fn option<'a>(args: &'a [String], name: &str) -> &'a str {
    let at = args.iter().position(|a| a == name).unwrap();
    &args[at + 1]
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

use FileType::{Gif, Mp4, Webp};

const PALETTE: &str = "split[a][b];[a]palettegen=max_colors=256:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle[out]";
const BOUNCE: &str = "split[f0][r0];[r0]reverse[r1];[f0][r1]concat=n=2:v=1:a=0";

#[test]
fn gif_fps_lanczos_scale_diff_palette_and_rectangle_paletteuse() {
    assert_eq!(
        plain(Gif),
        strings(&[
            "-i",
            "input.mp4",
            "-filter_complex",
            "[0:v]fps=15,scale=480:-2:flags=lanczos,split[a][b];[a]palettegen=max_colors=256:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle[out]",
            "-map",
            "[out]",
            "-loop",
            "0",
            "output.gif",
        ])
    );
}

#[test]
fn gif_colours_and_every_dithering_choice_reach_the_palette_filters() {
    let colors = command(Gif, |s| s.gif_colors = 64);
    assert!(graph(&colors).contains("palettegen=max_colors=64:stats_mode=diff"));
    for (dither, expected) in [
        (
            GifDither::FloydSteinberg,
            "paletteuse=dither=floyd_steinberg:diff_mode=rectangle",
        ),
        (
            GifDither::Bayer,
            "paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle",
        ),
        (
            GifDither::None,
            "paletteuse=dither=none:diff_mode=rectangle",
        ),
    ] {
        let args = command(Gif, |s| s.gif_dither = dither);
        assert!(graph(&args).contains(expected), "{dither:?}");
    }
}

#[test]
fn plays_become_gif_repeats_and_webp_loops() {
    for (plays, gif_loop, webp_loop) in
        [(0, "0", "0"), (1, "-1", "1"), (2, "1", "2"), (3, "2", "3")]
    {
        assert_eq!(
            option(&command(Gif, |s| s.plays = plays), "-loop"),
            gif_loop
        );
        assert_eq!(
            option(&command(Webp, |s| s.plays = plays), "-loop"),
            webp_loop
        );
    }
}

#[test]
fn webp_encodes_the_whole_animation_with_libwebp_anim_at_the_chosen_quality() {
    assert_eq!(
        command(Webp, |s| s.webp_quality = 63),
        strings(&[
            "-i",
            "input.mp4",
            "-filter_complex",
            "[0:v]fps=15,scale=480:-2:flags=lanczos[out]",
            "-map",
            "[out]",
            "-c:v",
            "libwebp_anim",
            "-lossless",
            "0",
            "-quality",
            "63",
            "-compression_level",
            "4",
            "-loop",
            "0",
            "output.webp",
        ])
    );
}

#[test]
fn mp4_libx264_crf_20_yuv420p_and_faststart_silent_by_default() {
    assert_eq!(
        plain(Mp4),
        strings(&[
            "-i",
            "input.mp4",
            "-filter_complex",
            "[0:v]fps=15,scale=480:-2:flags=lanczos,format=yuv420p[out]",
            "-map",
            "[out]",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "20",
            "-movflags",
            "+faststart",
            "-an",
            "output.mp4",
        ])
    );
}

#[test]
fn mp4_keeps_the_first_sound_track_only_when_asked() {
    let sound = Extra {
        keep_sound: true,
        ..EXTRA
    };
    let args = command_with(Mp4, |_| {}, sound);
    assert!(!args.contains(&"-an".to_owned()));
    assert_eq!(
        args[args.len() - 8..],
        strings(&[
            "-map",
            "0:a:0?",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-shortest",
            "output.mp4"
        ])
    );
    // Sound never reaches the image file types.
    assert!(!command_with(Gif, |_| {}, sound).contains(&"0:a:0?".to_owned()));
    assert!(!command_with(Webp, |_| {}, sound).contains(&"0:a:0?".to_owned()));
}

#[test]
fn every_type_trims_before_the_input_and_crops_before_fps_and_scale() {
    for file_type in [Gif, Webp, Mp4] {
        let args = command_with(
            file_type,
            |s| s.fps = 24,
            Extra {
                width: Some(320),
                trim: Some(InputTrim {
                    seek: 1.5,
                    duration: 2.5,
                }),
                crop: Some(Crop {
                    x: 0.25,
                    y: 0.1,
                    width: 0.5,
                    height: 0.8,
                }),
                ..EXTRA
            },
        );
        assert_eq!(
            args[..6],
            strings(&["-ss", "1.5", "-t", "2.5", "-i", "input.mp4"])
        );
        assert!(
            graph(&args).starts_with(
                "[0:v]crop='max(1,iw*0.5)':'max(1,ih*0.8)':iw*0.25:ih*0.1:exact=1,fps=24,scale=320:-2:flags=lanczos"
            ),
            "{file_type:?}: {}",
            graph(&args)
        );
    }
}

#[test]
fn an_unknown_width_keeps_the_cropped_width_made_even() {
    let args = command_with(
        Mp4,
        |_| {},
        Extra {
            width: None,
            ..EXTRA
        },
    );
    assert!(graph(&args).contains("scale=trunc(iw/2)*2:-2:flags=lanczos"));
}

#[test]
fn quarter_and_half_turns_come_before_fps_and_scale() {
    for (rotation, filter) in [
        (Rotation::Quarter, "transpose=1"),
        (Rotation::ThreeQuarters, "transpose=2"),
        (Rotation::Half, "hflip,vflip"),
    ] {
        assert_eq!(
            chain(&command(Webp, |s| s.rotation = rotation)),
            format!("{filter},fps=15,scale=480:-2:flags=lanczos[out]")
        );
    }
}

#[test]
fn flips_mirror_after_the_turn_hflip_then_vflip() {
    assert!(chain(&command(Webp, |s| s.flip_horizontal = true)).starts_with("hflip,fps="));
    assert!(chain(&command(Webp, |s| s.flip_vertical = true)).starts_with("vflip,fps="));
    let all = command(Webp, |s| {
        s.rotation = Rotation::Quarter;
        s.flip_horizontal = true;
        s.flip_vertical = true;
    });
    assert!(chain(&all).starts_with("transpose=1,hflip,vflip,fps="));
}

#[test]
fn speed_rescales_timestamps_before_fps_so_the_frame_rate_holds() {
    assert!(chain(&command(Webp, |s| s.speed = 2.0)).starts_with("setpts=PTS/2,fps=15,scale="));
    assert!(chain(&command(Webp, |s| s.speed = 0.25)).starts_with("setpts=PTS/0.25,fps="));
    assert!(!chain(&plain(Webp)).contains("setpts"));
}

#[test]
fn reverse_and_boomerang_come_after_scaling_before_each_encoder() {
    let reverse = |s: &mut Settings| s.direction = Direction::Reverse;
    let boomerang = |s: &mut Settings| s.direction = Direction::Boomerang;
    assert_eq!(
        chain(&command(Webp, reverse)),
        "fps=15,scale=480:-2:flags=lanczos,reverse[out]"
    );
    assert_eq!(
        chain(&command(Mp4, reverse)),
        "fps=15,scale=480:-2:flags=lanczos,reverse,format=yuv420p[out]"
    );
    assert!(
        chain(&command(Gif, reverse))
            .starts_with("fps=15,scale=480:-2:flags=lanczos,reverse,split[a][b];[a]palettegen")
    );
    assert_eq!(
        chain(&command(Webp, boomerang)),
        format!("fps=15,scale=480:-2:flags=lanczos,{BOUNCE}[out]")
    );
    assert_eq!(
        chain(&command(Mp4, boomerang)),
        format!("fps=15,scale=480:-2:flags=lanczos,{BOUNCE},format=yuv420p[out]")
    );
    assert!(chain(&command(Gif, boomerang)).contains(&format!(
        "scale=480:-2:flags=lanczos,{BOUNCE},split[a][b];[a]palettegen"
    )));
}

#[test]
fn every_control_together_follows_gif_studio_order() {
    let args = command_with(
        Gif,
        |s| {
            s.rotation = Rotation::ThreeQuarters;
            s.flip_horizontal = true;
            s.flip_vertical = true;
            s.speed = 1.5;
            s.fps = 24;
            s.direction = Direction::Boomerang;
        },
        Extra {
            width: Some(320),
            crop: Some(Crop {
                x: 0.25,
                y: 0.0,
                width: 0.5,
                height: 1.0,
            }),
            ..EXTRA
        },
    );
    assert_eq!(
        graph(&args),
        format!(
            "[0:v]crop='max(1,iw*0.5)':'max(1,ih*1)':iw*0.25:ih*0:exact=1,\
             transpose=2,hflip,vflip,setpts=PTS/1.5,fps=24,scale=320:-2:flags=lanczos,\
             {BOUNCE},{PALETTE}"
        )
    );
}

#[test]
fn mp4_sound_follows_the_speed_through_atempo() {
    for speed in [0.5, 0.67, 1.5, 2.0] {
        let args = command_with(
            Mp4,
            |s| s.speed = speed,
            Extra {
                keep_sound: true,
                ..EXTRA
            },
        );
        assert_eq!(
            args[args.len() - 10..],
            strings(&[
                "-map",
                "0:a:0?",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-af",
                &format!("atempo={speed}"),
                "-shortest",
                "output.mp4",
            ])
        );
    }
    let at_one = command_with(
        Mp4,
        |_| {},
        Extra {
            keep_sound: true,
            ..EXTRA
        },
    );
    assert!(at_one.contains(&"0:a:0?".to_owned()));
    assert!(!at_one.contains(&"-af".to_owned()));
}

#[test]
fn the_caption_image_is_the_second_input_after_the_trimmed_clip() {
    let args = command_with(
        Webp,
        |_| {},
        Extra {
            captions: true,
            trim: Some(InputTrim {
                seek: 1.0,
                duration: 2.0,
            }),
            ..EXTRA
        },
    );
    assert_eq!(
        args[..8],
        strings(&[
            "-ss",
            "1",
            "-t",
            "2",
            "-i",
            "input.mp4",
            "-i",
            "captions.png"
        ])
    );
}

#[test]
fn captions_overlay_after_the_turn_and_scale_before_the_direction() {
    let captioned = Extra {
        captions: true,
        ..EXTRA
    };
    let args = command_with(
        Webp,
        |s| {
            s.rotation = Rotation::Quarter;
            s.direction = Direction::Boomerang;
        },
        captioned,
    );
    assert_eq!(
        graph(&args),
        format!(
            "[0:v]transpose=1,fps=15,scale=480:-2:flags=lanczos[scaled];\
             [scaled][1:v]overlay=(W-w)/2:(H-h)/2,{BOUNCE}[out]"
        )
    );
    let args = command_with(Mp4, |s| s.direction = Direction::Reverse, captioned);
    assert_eq!(
        graph(&args),
        "[0:v]fps=15,scale=480:-2:flags=lanczos[scaled];\
         [scaled][1:v]overlay=(W-w)/2:(H-h)/2,reverse,format=yuv420p[out]"
    );
}

#[test]
fn a_gif_composites_captions_in_rgb_ahead_of_its_palette() {
    let args = command_with(
        Gif,
        |_| {},
        Extra {
            captions: true,
            ..EXTRA
        },
    );
    assert_eq!(
        graph(&args),
        format!(
            "[0:v]fps=15,scale=480:-2:flags=lanczos[scaled];\
             [scaled][1:v]overlay=(W-w)/2:(H-h)/2:format=rgb,{PALETTE}"
        )
    );
}

#[test]
fn sound_still_comes_from_the_clip_and_no_image_means_no_overlay() {
    let args = command_with(
        Mp4,
        |_| {},
        Extra {
            captions: true,
            keep_sound: true,
            ..EXTRA
        },
    );
    assert!(args.contains(&"0:a:0?".to_owned()));
    for file_type in [Gif, Webp, Mp4] {
        let args = plain(file_type);
        assert!(!args.contains(&"captions.png".to_owned()));
        assert!(!graph(&args).contains("overlay"));
    }
}

/// The arguments the app runs for `file_type` with `patch`: [`job_args`], without the runner's.
fn job(file_type: FileType, patch: impl FnOnce(&mut Settings)) -> Vec<String> {
    let args = build(file_type, patch, EXTRA, job_args);
    assert_eq!(args[..RUNNER_ARGS.len()], strings(&RUNNER_ARGS));
    args[RUNNER_ARGS.len()..].to_vec()
}

fn encoded(output: &str) -> Vec<String> {
    strings(&[
        "-stats_enc_pre:v:0",
        "pipe:1",
        "-stats_enc_pre_fmt:v:0",
        "encoded={t}",
        output,
    ])
}

const TAP_OUTPUT: [&str; 9] = [
    "-map",
    "[tap]",
    "-stats_enc_pre:v",
    "pipe:1",
    "-stats_enc_pre_fmt:v",
    "tapped={t}",
    "-f",
    "null",
    "-",
];

#[test]
fn every_run_reports_progress_on_stdout_and_never_overwrites() {
    for file_type in [Gif, Webp, Mp4] {
        let args = build(file_type, |_| {}, EXTRA, job_args);
        assert!(args.contains(&"-n".to_owned()));
        assert!(!args.contains(&"-progress".to_owned()));
    }
    // A streaming output is mgck's command with its encoder's frame times on stdout.
    let output = "output.webp";
    let mut expected = plain(Webp);
    expected.pop();
    expected.extend(encoded(output));
    assert_eq!(job(Webp, |_| {}), expected);
    let mp4 = job(Mp4, |s| s.direction = Direction::Boomerang);
    assert_eq!(
        graph(&mp4),
        graph(&command(Mp4, |s| s.direction = Direction::Boomerang))
    );
    assert!(mp4.ends_with(&encoded("output.mp4")));
}

#[test]
fn outputs_that_hold_frames_tap_the_selection_before_the_holding_filter() {
    let tapped = |args: &[String], output: &str| {
        let tap_at = args.len() - TAP_OUTPUT.len();
        assert_eq!(args[tap_at..], strings(&TAP_OUTPUT));
        assert_eq!(args[tap_at - 5..tap_at], encoded(output));
    };
    let reverse = |s: &mut Settings| s.direction = Direction::Reverse;
    let boomerang = |s: &mut Settings| s.direction = Direction::Boomerang;
    const HEAD: &str = "[0:v]fps=15,scale=480:-2:flags=lanczos,split[tap][main];[main]";

    let gif = job(Gif, |_| {});
    assert_eq!(graph(&gif), format!("{HEAD}{PALETTE}"));
    tapped(&gif, "output.gif");
    let gif = job(Gif, reverse);
    assert_eq!(graph(&gif), format!("{HEAD}reverse,{PALETTE}"));
    tapped(&gif, "output.gif");
    let gif = job(Gif, boomerang);
    assert_eq!(graph(&gif), format!("{HEAD}{BOUNCE},{PALETTE}"));
    tapped(&gif, "output.gif");
    let webp = job(Webp, reverse);
    assert_eq!(graph(&webp), format!("{HEAD}reverse[out]"));
    tapped(&webp, "output.webp");
    let mp4 = job(Mp4, reverse);
    assert_eq!(graph(&mp4), format!("{HEAD}reverse,format=yuv420p[out]"));
    tapped(&mp4, "output.mp4");
    // Captions are part of the selection the tap sees.
    let captioned = build(
        Gif,
        |_| {},
        Extra {
            captions: true,
            ..EXTRA
        },
        job_args,
    );
    assert!(graph(&captioned).contains("overlay=(W-w)/2:(H-h)/2:format=rgb,split[tap][main]"));
}

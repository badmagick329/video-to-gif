# Animated WebP through libwebp_anim

Status: Accepted (2026-11-24).

## Context

mgck makes animated WebP with `-c:v libwebp` through the `webp` muxer, because ffmpeg.wasm's core has no `libwebp_anim`. Each frame is then a complete lossy image. The user's native ffmpeg (gyan.dev 9.0.1 full build) has `libwebp_anim`, which hands the whole animation to libwebp's `WebPAnimEncoder`: frames store only the rectangle that changed and identical neighbours merge. Export (VTG-6) had to pick one route.

## Measurement

Three 4-second excerpts of real footage, filtered as export does (`fps=15`, lanczos scale to 480 wide), stored losslessly, then encoded by each route with mgck's options (`-lossless 0 -quality Q -compression_level 4 -loop 0`). Each output was decoded and compared frame by frame, aligned by time, against a lossless WebP of the same frames (so both sides go through libwebp's YUV-to-RGB step). Quality is luma SSIM and PSNR from RGB.

| Clip | Route | Q | Size | SSIM | PSNR |
| --- | --- | --- | --- | --- | --- |
| Talking head, static background (1080p) | libwebp | 80 | 1404 KB | 0.964 | 45.0 dB |
| | libwebp_anim | 80 | 59 KB | 0.962 | 44.5 dB |
| Stage performance, dark, fast motion | libwebp | 60 | 264 KB | 0.964 | 43.2 dB |
| | libwebp | 80 | 400 KB | 0.975 | 44.6 dB |
| | libwebp_anim | 80 | 344 KB | 0.956 | 43.9 dB |
| | libwebp_anim | 90 | 557 KB | 0.979 | 46.9 dB |
| Handheld clip, constant motion (942×576) | libwebp | 60 | 437 KB | 0.981 | 38.8 dB |
| | libwebp | 80 | 590 KB | 0.987 | 40.8 dB |
| | libwebp_anim | 80 | 514 KB | 0.984 | 40.2 dB |
| | libwebp_anim | 90 | 783 KB | 0.990 | 42.7 dB |

- Where much of the frame holds still, libwebp_anim is 24 times smaller at the same quality.
- Where everything moves, libwebp_anim is 13 to 14% smaller at the same `-quality`, and slightly lower in quality; per byte the two routes are within a few thousandths of SSIM of each other, libwebp a little ahead on the dark clip.
- Encoding took 0.4 to 1.5 s per clip on either route.
- `-loop` reaches the file's ANIM chunk on both routes (checked for 0, 1 and 3).

## Decision

WebP export uses `-c:v libwebp_anim`, keeping mgck's other options (`-lossless 0 -quality <webpQuality> -compression_level 4 -loop <plays>`). The argument builder is `single_pass_args` in [args.rs](../../src-tauri/src/export/domain/args.rs).

## Consequences

- Clips with still regions (screen recordings, talking heads, memes cut from static shots) come out far smaller; moving footage costs about the same per unit of quality as before.
- The WebP quality slider means slightly less on moving footage than in mgck: the same number gives a slightly smaller, slightly softer file.
- The app needs an ffmpeg built with `libwebp_anim`. Common Windows builds (gyan.dev, BtbN) include it; the setup check does not test for it, so a build without it fails the WebP output with ffmpeg's "Unknown encoder" error.
- libwebp_anim merges identical neighbouring frames, so a WebP can hold fewer frames than the GIF of the same export while playing for the same time.

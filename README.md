# video-to-gif

A Windows desktop app that turns a video into a GIF, WebP or MP4. It reproduces the advanced mode of [mgck's Discord GIF tool](https://mgck.ink/discordgifs): trim, crop, resize, change the frame rate and speed, play forward, reversed or as a boomerang, rotate and flip, add captions, and tune GIF colours, WebP quality and MP4 sound. It can save any mix of the three formats in one run.

The browser version runs ffmpeg as WebAssembly and fails on AV1, HEVC and MKV files and on multi-GB recordings. This app runs your own ffmpeg instead, so it opens anything your ffmpeg can read, at any size, straight from where the file sits on disk.

It is early work. The app is being built now and does not convert anything yet.

## Requirements

- Windows 10 or 11 with WebView2 (Windows 11 ships it).
- `ffmpeg` and `ffprobe` on your PATH. The app shows a setup message when either is missing. Check with `where.exe ffmpeg` and `where.exe ffprobe`.

To build it you also need stable Rust, Node.js and pnpm.

## Development

```sh
pnpm install     # install frontend dependencies
pnpm tauri dev   # run the app with hot reload
pnpm check       # formatting, lints and tests for Rust and TypeScript
```

`pnpm check` stops at the first failure. It runs `cargo fmt --check`, Clippy with warnings as errors, `cargo test`, `tsc --noEmit`, ESLint and Vitest.

The code is Tauri 2: a Rust backend that runs ffmpeg, and a React and TypeScript frontend. [docs/](docs/README.md) holds the architecture notes and design decisions.

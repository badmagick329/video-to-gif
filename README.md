# video-to-gif

A Windows desktop app that turns a video into a GIF, WebP or MP4. You can trim, crop, resize, change the frame rate and speed, play forward, reversed or as a boomerang, rotate and flip, add captions, and tune GIF colours (or make the GIF with gifski), WebP quality and MP4 sound, then save any mix of the three formats in one run. The settings started as the advanced mode of [mgck's Discord GIF tool](https://mgck.ink/discordgifs).

The browser version runs ffmpeg as WebAssembly and fails on AV1, HEVC and MKV files and on multi-GB recordings. This app runs your own ffmpeg instead, so it opens anything your ffmpeg can read, at any size, straight from where the file sits on disk.

It is an early version (0.1.0) with rough edges. To open a video, drop it anywhere on the window, press Ctrl+O, or pick it from Recent, which lists the last eight videos you opened. The window is laid out like a video editor, with the source and the output playing side by side and every setting in a panel on the right. Before you export, it shows roughly how big each format will be, worked out from short test encodes of your selection that run in the background. Export saves next to the video or in a folder you pick, and the Exports tab shows how far along each format is while it runs. It then lists each file it made with buttons to open it, show it in Explorer, or copy it so you can paste the file into a folder or a chat.

## Requirements

- Windows 10 or 11 with WebView2 (Windows 11 ships it).
- `ffmpeg` and `ffprobe` 6.1 or newer on your PATH. The app shows a setup message when either is missing or ffmpeg is too old. Check with `where.exe ffmpeg` and `where.exe ffprobe`.
- Optional: [gifski](https://gif.ski) on your PATH (tested with 1.32.0, `cargo install gifski`). When the app finds it at startup, the GIF encoder menu offers gifski for higher-quality GIFs, and the status bar shows its version. Without it, GIFs are made by ffmpeg.

To build it you also need stable Rust, Node.js and pnpm.

## Install

Run `video-to-gif_0.1.0_x64-setup.exe`. It installs for your Windows account only, so it never asks for administrator rights. The app goes into `%LOCALAPPDATA%\video-to-gif` and gets a Start menu entry. On a PC without WebView2, the installer downloads it first.

The installer isn't signed yet, so the first time you run it Windows SmartScreen shows "Windows protected your PC". Click *More info*, then *Run anyway*.

To uninstall, open Settings > Apps > Installed apps and remove video-to-gif.

## Development

```sh
pnpm install     # install frontend dependencies
pnpm tauri dev   # run the app with hot reload
pnpm check       # formatting, lints and tests for Rust and TypeScript
pnpm tauri build # build the installer
```

`pnpm check` stops at the first failure. It runs `cargo fmt --check`, Clippy with warnings as errors, `cargo test`, `tsc --noEmit`, ESLint and Vitest.

The code is Tauri 2: a Rust backend that runs ffmpeg, and a React and TypeScript frontend. [docs/](docs/README.md) holds the architecture notes and design decisions, and [docs/context/releasing.md](docs/context/releasing.md) covers versioning, the icon and the installer build.

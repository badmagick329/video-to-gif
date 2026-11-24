# Architecture

How the app is put together, section by section. Only what is decided and built is here; each VTG Task adds the sections for what it builds. Terms are in [glossary.md](glossary.md).

## Shape

One repo, two halves, joined by Tauri 2 ([why](../decisions/2026-11-24-tauri-2.md)):

- **Frontend**, at the repo root: a pnpm package built by Vite ([package.json](../../package.json), [vite.config.ts](../../vite.config.ts), [tsconfig.json](../../tsconfig.json), Tailwind 3 through [tailwind.config.js](../../tailwind.config.js) and [postcss.config.js](../../postcss.config.js), [eslint.config.js](../../eslint.config.js), [index.html](../../index.html)). React 18 + TypeScript code is in `src/`. It runs in the system webview, WebView2 on Windows. The editor UI is copied from mgck ([why](../decisions/2026-11-24-ui-copied-from-mgck.md)) and lands with VTG-5.
- **Backend**, in `src-tauri/`: the standard Tauri 2 crate ([Cargo.toml](../../src-tauri/Cargo.toml), [tauri.conf.json](../../src-tauri/tauri.conf.json)). Identifier `ink.mgck.video-to-gif`, product name and window title `video-to-gif`.

`pnpm tauri dev` runs Vite's dev server and the Rust app together; the dev server only feeds the webview during development. The shipped app has no localhost server: the frontend reaches Rust only through IPC (*IPC boundary*).

## Slices

The Rust crate is vertical slices, one folder per feature under `src-tauri/src/`, each split into the layers it uses ([why, and when a layer is added](../decisions/2026-11-24-vertical-slices.md)):

| Slice | Layers | Owns |
| --- | --- | --- |
| [`probe`](../../src-tauri/src/probe/mod.rs) | domain, infrastructure, presentation | Finding `ffmpeg` and `ffprobe` (*Finding ffmpeg*), and reading a source's metadata with ffprobe (*Probing a source*). |
| [`preview`](../../src-tauri/src/preview/mod.rs) | | Deciding whether WebView2 can play the source, and making a proxy when it cannot ([why](../decisions/2026-11-24-preview-source-or-proxy.md)). |
| [`export`](../../src-tauri/src/export/mod.rs) | | Building ffmpeg arguments from the selection, crop and settings, running the jobs, streaming progress, cancelling, reporting outputs. |

`preview` and `export` are so far only their `mod.rs`, saying what each owns. A layer appears with the first code that needs it: `domain` (types and rules, no I/O), `application` (use cases that coordinate infrastructure), `infrastructure` (processes, filesystem), `presentation` (the slice's `#[tauri::command]`s and the events it emits). Fill in a row's *Layers* when one appears.

- **Dependencies point inward.** [lib.rs](../../src-tauri/src/lib.rs) is the shell: it builds the Tauri app and is the only place that registers every slice's commands. Within a slice, outer layers use inner ones. Across slices, only `domain` is shared, and no slice imports the shell.
- **Module paths name the layer** (`crate::probe::domain::…`), with no re-exports, so a wrong-way import shows in the `use` line.

## IPC boundary

Rust owns ffmpeg ([why](../decisions/2026-11-24-rust-owns-ffmpeg.md)). The frontend sends intent: the source path, the selection, the crop and the output settings, through `invoke` on a slice's command. Rust builds the ffmpeg and ffprobe arguments, runs the processes, and streams progress back as Tauri events. The frontend never builds a command line.

Everything crossing IPC from the frontend is a trust boundary: a slice's `presentation` validates it into `domain` types (paths exist, times and crop lie inside the source, settings are in range) before any process starts. ffprobe's output is another boundary, parsed and checked where it is read. Inside those boundaries, code trusts the domain types and fails fast.

Sources arrive as real filesystem paths, by native drag-drop or a file dialog, and are never copied ([why](../decisions/2026-11-24-real-paths.md)).

## Finding ffmpeg

v0 runs the user's own `ffmpeg` and `ffprobe`, found on PATH, and shows the setup message when either cannot run ([why](../decisions/2026-11-24-users-ffmpeg.md)).

- **Lookup** ([search_path.rs](../../src-tauri/src/probe/infrastructure/search_path.rs)): PATH directories in order, and within each the PATHEXT extensions in order, matching regular files only, as `where.exe` does. `ffprobe` is looked for beside the resolved `ffmpeg` first, then on PATH, so both come from one build. The search reads the process's PATH on every call; nothing is cached, so a user who fixes PATH restarts the app.
- **Startup check**: the `find_ffmpeg` command returns each tool as `found` (path and version from `-version`), `missing`, or `unusable` (a file that failed `-version`). [App.tsx](../../src/App.tsx) calls it on mount and renders [SetupMessage.tsx](../../src/SetupMessage.tsx) in place of the app unless both are `found`.
- **Spawning**: every ffmpeg and ffprobe process starts through [process.rs](../../src-tauri/src/probe/infrastructure/process.rs), which sets `CREATE_NO_WINDOW` on Windows (a GUI app's console child would flash a window) and a null stdin. `preview` and `export` need the same flags, but slices share only `domain`, so where their spawning lives is decided when the first of them is built.

The owner's machine has two ffmpeg sets on PATH. Windows resolves `C:\scripts\` first (ffmpeg 9.0.1, gyan.dev full build), then the winget `yt-dlp.FFmpeg` nightly. `C:\scripts\` also holds a folder named `ffmpeg`, which is why Git Bash's `which ffmpeg` reports the winget copy; check with `where.exe ffmpeg`, which resolves as the app does. A test asserts the app's lookup matches `where.exe`'s first line.

To see the setup message, launch `src-tauri\target\debug\video-to-gif.exe` (from `pnpm tauri build --debug --no-bundle`) from a shell whose PATH drops every directory holding `ffmpeg.exe` or `ffprobe.exe`.

## Probing a source

The `probe_source` command takes an absolute path to an existing file and returns [source info](glossary.md), or a `ProbeError` tagged by `kind`: `ffprobeMissing`, `ffprobeFailed`, `notFound`, `unreadable` (ffprobe's last stderr line as `reason`), `noVideoStream`, `invalidOutput`. Types are in [domain.rs](../../src-tauri/src/probe/domain.rs), mirrored for TypeScript in [probe.ts](../../src/probe.ts); change both together.

[ffprobe/mod.rs](../../src-tauri/src/probe/infrastructure/ffprobe/mod.rs) runs `ffprobe -v error -hide_banner -show_format -show_streams -of json <path>` and validates the JSON:

- `-show_streams`/`-show_format` rather than `-show_entries`, because entry names such as `stream_side_data` differ between ffprobe versions and older ones reject them (gif-studio 1.1.1's fix).
- The video stream is the first one not flagged `attached_pic`, so cover art is skipped.
- Width and height are the displayed size: rotation comes from the display matrix in `side_data_list`, else the legacy `rotate` tag, and a quarter turn swaps the coded size. ffmpeg auto-rotates before filters, so crop rectangles use these pixels.
- Frame rate is `avg_frame_rate`, else `r_frame_rate`; values outside (0, 1000] count as absent. Duration is the format's, else the stream's; it must be positive.
- `container` is ffprobe's `format_name` as given, a comma-separated demuxer list (`mov,mp4,m4a,3gp,3g2,mj2`, `matroska,webm`).

Tests run the real ffmpeg on PATH: they generate short `testsrc2` clips in temp dirs (H.264 MP4, AV1 MP4, HEVC MKV, VP9 WebM, no audio, 90° display rotation, audio only, a non-media file) and probe them, so `cargo test` needs ffmpeg with libx264, libsvtav1, libx265, libvpx-vp9 and libopus.

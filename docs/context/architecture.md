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
| [`probe`](../../src-tauri/src/probe/mod.rs) | | Finding `ffmpeg` and `ffprobe` (*Finding ffmpeg*), and reading a source's metadata with ffprobe. |
| [`preview`](../../src-tauri/src/preview/mod.rs) | | Deciding whether WebView2 can play the source, and making a proxy when it cannot ([why](../decisions/2026-11-24-preview-source-or-proxy.md)). |
| [`export`](../../src-tauri/src/export/mod.rs) | | Building ffmpeg arguments from the selection, crop and settings, running the jobs, streaming progress, cancelling, reporting outputs. |

Each slice is so far only its `mod.rs`, saying what it owns. A layer appears with the first code that needs it: `domain` (types and rules, no I/O), `application` (use cases that coordinate infrastructure), `infrastructure` (processes, filesystem), `presentation` (the slice's `#[tauri::command]`s and the events it emits). Fill in a row's *Layers* when one appears.

- **Dependencies point inward.** [lib.rs](../../src-tauri/src/lib.rs) is the shell: it builds the Tauri app and is the only place that registers every slice's commands. Within a slice, outer layers use inner ones. Across slices, only `domain` is shared, and no slice imports the shell.
- **Module paths name the layer** (`crate::probe::domain::…`), with no re-exports, so a wrong-way import shows in the `use` line.

## IPC boundary

Rust owns ffmpeg ([why](../decisions/2026-11-24-rust-owns-ffmpeg.md)). The frontend sends intent: the source path, the selection, the crop and the output settings, through `invoke` on a slice's command. Rust builds the ffmpeg and ffprobe arguments, runs the processes, and streams progress back as Tauri events. The frontend never builds a command line.

Everything crossing IPC from the frontend is a trust boundary: a slice's `presentation` validates it into `domain` types (paths exist, times and crop lie inside the source, settings are in range) before any process starts. ffprobe's output is another boundary, parsed and checked where it is read. Inside those boundaries, code trusts the domain types and fails fast.

Sources arrive as real filesystem paths, by native drag-drop or a file dialog, and are never copied ([why](../decisions/2026-11-24-real-paths.md)).

## Finding ffmpeg

v0 runs the user's own `ffmpeg` and `ffprobe`, found on PATH, and shows a setup message when either is missing ([why](../decisions/2026-11-24-users-ffmpeg.md)). The `probe` slice owns the lookup; VTG-3 builds it.

The owner's machine has two ffmpeg sets on PATH. Windows resolves `C:\scripts\` first (ffmpeg 9.0.1, gyan.dev full build), then the winget `yt-dlp.FFmpeg` nightly. Git Bash's `which ffmpeg` misreports which one wins; check with `where.exe ffmpeg` or with Rust's `std::process::Command`, which resolve as the app does.

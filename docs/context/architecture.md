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
| [`preview`](../../src-tauri/src/preview/mod.rs) | domain, infrastructure, presentation | Opening a source for the player, deciding whether WebView2 plays it or a proxy, building proxies, and the temp folder they live in (*Previewing a source*). |
| [`export`](../../src-tauri/src/export/mod.rs) | | Building ffmpeg arguments from the selection, crop and settings, running the jobs, streaming progress, cancelling, reporting outputs. |

`export` is so far only its `mod.rs`, saying what it owns. A layer appears with the first code that needs it: `domain` (types and rules, no I/O), `application` (use cases that coordinate infrastructure), `infrastructure` (processes, filesystem), `presentation` (the slice's `#[tauri::command]`s and the events it emits). Fill in a row's *Layers* when one appears.

- **Dependencies point inward.** [lib.rs](../../src-tauri/src/lib.rs) is the shell: it builds the Tauri app, is the only place that registers every slice's commands, and wires slices into app startup and exit. Within a slice, outer layers use inner ones. Across slices, only `domain` and the shared kernel are shared, and no slice imports the shell.
- **Shared kernel**: [process.rs](../../src-tauri/src/process.rs) and [search_path.rs](../../src-tauri/src/search_path.rs), crate-level modules any slice's `infrastructure` uses to find and start ffmpeg and ffprobe ([why](../decisions/2026-11-24-vertical-slices.md#amendment-2026-11-24-a-shared-kernel-for-spawning-ffmpeg)). They import no slice.
- **Module paths name the layer** (`crate::probe::domain::…`), with no re-exports, so a wrong-way import shows in the `use` line.

## IPC boundary

Rust owns ffmpeg ([why](../decisions/2026-11-24-rust-owns-ffmpeg.md)). The frontend sends intent: the source path, the selection, the crop and the output settings, through `invoke` on a slice's command. Rust builds the ffmpeg and ffprobe arguments, runs the processes, and streams progress back as Tauri events. The frontend never builds a command line.

Everything crossing IPC from the frontend is a trust boundary: a slice's `presentation` validates it into `domain` types (paths exist, times and crop lie inside the source, settings are in range) before any process starts. ffprobe's output is another boundary, parsed and checked where it is read. Inside those boundaries, code trusts the domain types and fails fast.

Sources arrive as real filesystem paths, by native drag-drop or a file dialog, and are never copied ([why](../decisions/2026-11-24-real-paths.md)).

## Finding ffmpeg

v0 runs the user's own `ffmpeg` and `ffprobe`, found on PATH, and shows the setup message when either cannot run ([why](../decisions/2026-11-24-users-ffmpeg.md)).

- **Lookup** ([search_path.rs](../../src-tauri/src/search_path.rs)): PATH directories in order, and within each the PATHEXT extensions in order, matching regular files only, as `where.exe` does. `ffprobe` is looked for beside the resolved `ffmpeg` first, then on PATH, so both come from one build. The search reads the process's PATH on every call; nothing is cached, so a user who fixes PATH restarts the app.
- **Startup check**: the `find_ffmpeg` command returns each tool as `found` (path and version from `-version`), `missing`, or `unusable` (a file that failed `-version`). [App.tsx](../../src/App.tsx) calls it on mount and renders [SetupMessage.tsx](../../src/SetupMessage.tsx) in place of the app unless both are `found`.
- **Spawning**: every ffmpeg and ffprobe process, in any slice, starts through [process.rs](../../src-tauri/src/process.rs), which sets `CREATE_NO_WINDOW` on Windows (a GUI app's console child would flash a window) and a null stdin.

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

## Previewing a source

The user opens a source by dropping it on the window (Tauri's native drag-drop, `onDragDropEvent`; of several files the first opens) or with **Open video…** (`tauri-plugin-dialog`, permission `dialog:allow-open`). Either way the frontend gets a path and [SourcePreview.tsx](../../src/SourcePreview.tsx) runs `probe_source`, then `open_source`, then plays the source or calls `make_proxy`. Results for an open that a newer one replaced are dropped. IPC types are in [preview/domain.rs](../../src-tauri/src/preview/domain.rs), mirrored in [preview.ts](../../src/preview.ts); change both together.

| IPC | Shape |
| --- | --- |
| `open_source(path, info)` command | `info` is the `SourceInfo` that `probe_source` returned, rechecked as untrusted. Returns `"source"` or `"proxy"`. Replacing the open source cancels its proxy build and deletes its proxy; reopening the same path keeps it. |
| `make_proxy(path)` command | Returns the proxy's path once built, or the existing one. `path` must be the open source. |
| `proxy-progress` event | `{ source, progress }`, progress 0..1 from ffmpeg's `out_time_us` over the probed duration. |
| `PreviewError` | Tagged by `kind`: `notFound`, `invalidSourceInfo`, `notServable`, `notOpen`, `ffmpegMissing`, `proxyFailed` (ffmpeg's last stderr line as `reason`), `cancelled` (the frontend ignores it). |

**Source or proxy** ([why](../decisions/2026-11-24-preview-source-or-proxy.md)): `choose_preview_source` plays the source when its codec is H.264, VP9 or AV1 and its container is MP4 (`format_name` lists `mp4`, which also covers MOV) or WebM. ffprobe names MKV and WebM alike (`matroska,webm`), so Matroska counts as WebM only for a `.webm` file; MKV takes the proxy. HEVC always takes the proxy, because WebView2 decodes it only with the HEVC Video Extensions and a capable GPU. Each allowed codec was confirmed on the owner's machine by playing generated clips in the app's WebView2 (decoded frames counted, seeks landing). H.264, VP9 and AV1 in MKV, and HEVC, also played there, so those exclusions guard other PCs and MKV-only features (FLAC, PCM, ordered chapters); they are not observed failures. When the player raises `error` on the source, or shows no picture (`videoWidth` 0), the frontend switches to a proxy.

**Proxy** (`proxy_args` in [preview/domain.rs](../../src-tauri/src/preview/domain.rs)): `-map 0:V:0` (skips cover art) plus `-map 0:a:0` when the source has audio; `scale` to at most 720 high from the displayed size, never upscaled, both sides even; `-fps_mode passthrough`; `libx264 -preset ultrafast -crf 23`; `-force_key_frames expr:gte(t,n_forced)` (a keyframe a second, for scrubbing); stereo AAC 128k; `-movflags +faststart`; `-progress pipe:1`. There is no `-ss`, `-copyts` or `-r`, so the proxy starts at 0 at the source's first timestamp and time t in the player is t after the source's start, the offset export's input `-ss` measures (a test checks this on a source starting at 2.5 s). For export: the player at t shows the frame whose span holds t, while input `-ss t` starts at the first frame at or after t, up to one frame later. A 2.4 GB 4K H.264/FLAC MKV of 187 s took about a minute to proxy on the owner's machine.

**Asset protocol**: the webview loads sources and proxies from `http://asset.localhost/<path>` (`convertFileSrc`). [tauri.conf.json](../../src-tauri/tauri.conf.json) enables the protocol (Cargo feature `protocol-asset`) with an empty scope; [preview/presentation.rs](../../src-tauri/src/preview/presentation.rs) allows this session's temp folder at startup and each source file as it opens, and any other path gets 403. Tauri's scope cannot shrink, so a session's earlier sources stay readable until exit. Range requests get 206 with at most 1000 KB per response, and the `<video>` element always sends `Range`: a 2.75 GB file opened and seeked with the app's memory under 50 MB. A request without `Range` reads the whole file into memory, so anything else fetching a source sends one.

**Temp folder** ([temp_folder.rs](../../src-tauri/src/preview/infrastructure/temp_folder.rs)): `%TEMP%\video-to-gif\session-<pid>\` holds `proxy-<n>.mp4` files and a `lock` file the running app keeps locked. At startup the app deletes every entry in `%TEMP%\video-to-gif` except session folders whose lock another running copy holds, so a crashed session's proxies go and a second running app's stay. A proxy lives while its source stays open; [session.rs](../../src-tauri/src/preview/infrastructure/session.rs) deletes it when another source replaces it. On `RunEvent::Exit` (after the last window closes) the app kills a running proxy encode, waits for ffmpeg to exit so no file stays locked, then deletes its session folder and, when nothing else is in it, `%TEMP%\video-to-gif`.

To drive the preview in the real app, launch the debug build with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` and evaluate JS over CDP (`http://127.0.0.1:9222/json`); `window.__TAURI_INTERNALS__.invoke` reaches the commands. The debug build opens a console window beside the app; the release build does not.

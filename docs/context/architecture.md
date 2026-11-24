# Architecture

How the app is put together, section by section. Only what is decided and built is here; each VTG Task adds the sections for what it builds. Terms are in [glossary.md](glossary.md).

## Shape

One repo, two halves, joined by Tauri 2 ([why](../decisions/2026-11-24-tauri-2.md)):

- **Frontend**, at the repo root: a pnpm package built by Vite ([package.json](../../package.json), [vite.config.ts](../../vite.config.ts), [tsconfig.json](../../tsconfig.json), Tailwind 3 through [tailwind.config.js](../../tailwind.config.js) and [postcss.config.js](../../postcss.config.js), [eslint.config.js](../../eslint.config.js), [index.html](../../index.html)). React 18 + TypeScript code is in `src/`. It runs in the system webview, WebView2 on Windows. Its layout is in *Frontend*.
- **Backend**, in `src-tauri/`: the standard Tauri 2 crate ([Cargo.toml](../../src-tauri/Cargo.toml), [tauri.conf.json](../../src-tauri/tauri.conf.json)). Identifier `ink.mgck.video-to-gif`, product name and window title `video-to-gif`.

`pnpm tauri dev` runs Vite's dev server and the Rust app together; the dev server only feeds the webview during development. The shipped app has no localhost server: the frontend reaches Rust only through IPC (*IPC boundary*).

## Frontend

The editor is mgck's advanced mode, copied and adapted ([why](../decisions/2026-11-24-ui-copied-from-mgck.md)); mgck paths below are under `C:\code\python\web\mgck\frontend\src`.

- [main.tsx](../../src/main.tsx) imports the bundled fonts; [App.tsx](../../src/App.tsx) runs the startup check (*Finding ffmpeg*) and renders [SetupMessage.tsx](../../src/SetupMessage.tsx) or the editor.
- `src/ipc/`: one module per Rust slice ([probe.ts](../../src/ipc/probe.ts), [preview.ts](../../src/ipc/preview.ts), [export.ts](../../src/ipc/export.ts)), mirroring that slice's IPC types and wrapping its commands. A new slice's commands get a module here.
- `src/editor/`: advanced mode. Components and hooks sit flat in the folder; `lib/` holds the logic without React; `__tests__/` holds the tests, with [tauri.ts](../../src/editor/__tests__/tauri.ts) standing in for the Tauri APIs and [harness.tsx](../../src/editor/__tests__/harness.tsx) rendering the editor with a spy for Convert. A later feature that is not part of the editor gets its own folder beside it.

| Here | From mgck | Changed |
| --- | --- | --- |
| `editor/Editor.tsx` | `app/discordgifs/_components/FileDropzone.tsx`, advanced branch | Holds the clip's trim, crop and settings; opening comes from `useSource`. No mode switch, paste, page chrome or metadata. |
| `editor/useSource.ts` | (VTG-4's `SourcePreview.tsx`) | Drop, file dialog, probe, preview source or proxy, fallback to a proxy when the player fails. Replaces mgck's dropzone and paste hooks. |
| `editor/AdvancedEditor.tsx` | `_components/AdvancedEditor.tsx` | Sizes, length and sound from source info; shows proxy progress in the player's place; Convert builds an `ExportRequest` (`lib/types.ts`, the `run_export` payload) and hands it to a stub; VTG-7 wires it to `runExport` with a destination, progress, cancel and the memory warning. |
| `editor/VideoTrimmer.tsx` | `_components/VideoTrimmer.tsx` | Plays the preview source's URL; size, duration and frame step from source info; reports an undecodable player upward. Simple mode's preset previews and sticker button dropped. |
| `editor/VideoCropper.tsx`, `TrimTimeline.tsx`, `ClipTransport.tsx`, `AdvancedPreview.tsx`, `OutputPreview.tsx`, `AdvancedSettingsForm.tsx`, `SessionResults.tsx` | same names in `_components/` | Imports, and the sound switch shows whenever the probe found sound. `SessionResults` stays empty until export (VTG-7). |
| `editor/Switch.tsx` | `components/ui/switch.tsx` (shadcn) | `clsx` for `cn`, -dg colours for shadcn's tokens. |
| `editor/useClipShortcuts.ts`, `useTimelineThumbnails.ts` | `hooks/discordgifs/` | Thumbnails come from `make_thumbnails` (*Previewing a source*), not from seeking a hidden `<video>`. |
| `editor/lib/advanced.ts`, `captions.ts`, `crop.ts`, `output-preview.ts`, `shortcuts.ts`, `trim.ts`, `video-frames.ts`, `types.ts` | `lib/discordgifs/`, `lib/types/discordgifs.ts` | Wasm frame-memory limits, `clipHasAudio`, download names, simple-mode presets and the assumed 30 fps dropped; captions use the bundled Anton. |

Pixel figures always use the source's displayed size from the probe, never the player's: a proxy is smaller. Crops are stored as fractions of the frame (`ClipCrop`), so the box drawn over a proxy is the same box on the source; the crop readout, the full-frame check, width caps and output size multiply by the displayed size, which is what export crops. The output preview draws from whatever the player shows, using the same fractions.

mgck turns settings into ffmpeg arguments in `lib/discordgifs/single-pass-commands.ts` and `ffmpeg-commands.ts`, run by `ffmpeg-manager.ts`. None of these were copied: export builds arguments in Rust ([why](../decisions/2026-11-24-rust-owns-ffmpeg.md)), ported from them (*Exporting*). Captions are the exception that stays in the frontend: Convert renders them to a PNG at the exact output size (`renderCaptionImage`), the same drawing the preview shows, for export to overlay.

**Theme**: [tailwind.config.js](../../tailwind.config.js) and [index.css](../../src/index.css) carry mgck's `-dg` colours, glow shadows and radius, light and dark, switched by `prefers-color-scheme` as mgck's default system theme does. Quicksand (UI, weights 300 to 700) and Anton (captions) come from `@fontsource` packages that Vite bundles, so the app fetches no font at runtime.

**Checking it in a browser**: `pnpm dev` serves any HTML file under the repo root, so a page in a gitignored folder (e.g. `src-tauri/target/`) that defines `window.__TAURI_INTERNALS__` (`invoke`, `transformCallback`, `convertFileSrc`, `metadata`) before loading `/src/main.tsx` runs the editor in Chrome against clips served from the same folder. That is how the editor was compared with mgck's advanced mode control by control.

## Slices

The Rust crate is vertical slices, one folder per feature under `src-tauri/src/`, each split into the layers it uses ([why, and when a layer is added](../decisions/2026-11-24-vertical-slices.md)):

| Slice | Layers | Owns |
| --- | --- | --- |
| [`probe`](../../src-tauri/src/probe/mod.rs) | domain, infrastructure, presentation | Finding `ffmpeg` and `ffprobe` (*Finding ffmpeg*), and reading a source's metadata with ffprobe (*Probing a source*). |
| [`preview`](../../src-tauri/src/preview/mod.rs) | domain, infrastructure, presentation | Opening a source for the player, deciding whether WebView2 plays it or a proxy, building proxies and timeline thumbnails, and the temp folder they live in (*Previewing a source*). |
| [`export`](../../src-tauri/src/export/mod.rs) | domain, application, infrastructure, presentation | Building ffmpeg arguments from the selection, crop and settings, running the jobs, streaming progress, cancelling, putting outputs in the destination folder, and the frame-memory guard (*Exporting*). |

A layer appears with the first code that needs it: `domain` (types and rules, no I/O), `application` (use cases that coordinate infrastructure), `infrastructure` (processes, filesystem), `presentation` (the slice's `#[tauri::command]`s and the events it emits). Fill in a row's *Layers* when one appears.

- **Dependencies point inward.** [lib.rs](../../src-tauri/src/lib.rs) is the shell: it builds the Tauri app, is the only place that registers every slice's commands, and wires slices into app startup and exit. Within a slice, outer layers use inner ones. Across slices, only `domain` and the shared kernel are shared, and no slice imports the shell.
- **Shared kernel**: crate-level modules any slice's `infrastructure` may use. [process.rs](../../src-tauri/src/process.rs) and [search_path.rs](../../src-tauri/src/search_path.rs) find and start ffmpeg and ffprobe ([why](../decisions/2026-11-24-vertical-slices.md#amendment-2026-11-24-a-shared-kernel-for-spawning-ffmpeg)); [temp_folder.rs](../../src-tauri/src/temp_folder.rs) is the session temp folder ([why](../decisions/2026-11-24-vertical-slices.md#amendment-2026-11-24-the-temp-folder-joins-the-shared-kernel)). They import no slice.
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

The `probe_source` command takes an absolute path to an existing file and returns [source info](glossary.md), or a `ProbeError` tagged by `kind`: `ffprobeMissing`, `ffprobeFailed`, `notFound`, `unreadable` (ffprobe's last stderr line as `reason`), `noVideoStream`, `invalidOutput`. Types are in [domain.rs](../../src-tauri/src/probe/domain.rs), mirrored for TypeScript in [ipc/probe.ts](../../src/ipc/probe.ts); change both together.

[ffprobe/mod.rs](../../src-tauri/src/probe/infrastructure/ffprobe/mod.rs) runs `ffprobe -v error -hide_banner -show_format -show_streams -of json <path>` and validates the JSON:

- `-show_streams`/`-show_format` rather than `-show_entries`, because entry names such as `stream_side_data` differ between ffprobe versions and older ones reject them (gif-studio 1.1.1's fix).
- The video stream is the first one not flagged `attached_pic`, so cover art is skipped.
- Width and height are the displayed size: rotation comes from the display matrix in `side_data_list`, else the legacy `rotate` tag, and a quarter turn swaps the coded size. ffmpeg auto-rotates before filters, so crop rectangles use these pixels.
- Frame rate is `avg_frame_rate`, else `r_frame_rate`; values outside (0, 1000] count as absent. Duration is the format's, else the stream's; it must be positive.
- `container` is ffprobe's `format_name` as given, a comma-separated demuxer list (`mov,mp4,m4a,3gp,3g2,mj2`, `matroska,webm`).
- `audioCodec` is the first audio stream's `codec_name` (the stream the player and the proxy use), `"unknown"` when ffprobe names none, and `null` when there is no audio. It is the only field saying whether there is sound; TypeScript reads it through `hasAudio(info)`.

Tests run the real ffmpeg on PATH: they generate short `testsrc2` clips in temp dirs (H.264 MP4, AV1 MP4, HEVC MKV, VP9 WebM, no audio, 90° display rotation, audio only, a non-media file, and FLAC, Vorbis, MP3, Opus and AC-3 audio) and probe them, so `cargo test` needs ffmpeg with libx264, libsvtav1, libx265, libvpx-vp9, libopus, libvorbis and libmp3lame.

## Previewing a source

The user opens a source by dropping it anywhere on the window (Tauri's native drag-drop, `onDragDropEvent`; of several files the first opens), or by clicking the editor's dropzone or pressing Ctrl+O, which open `tauri-plugin-dialog`'s file dialog (permission `dialog:allow-open`). Either way the frontend gets a path and [useSource.ts](../../src/editor/useSource.ts) runs `probe_source`, then `open_source`, then plays the source or calls `make_proxy`. Results for an open that a newer one replaced are dropped. IPC types are in [preview/domain.rs](../../src-tauri/src/preview/domain.rs), mirrored in [ipc/preview.ts](../../src/ipc/preview.ts); change both together.

| IPC | Shape |
| --- | --- |
| `open_source(path, info)` command | `info` is the `SourceInfo` that `probe_source` returned, rechecked as untrusted. Returns `"source"` or `"proxy"`. Replacing the open source cancels its proxy build and deletes its proxy; reopening the same path keeps it. |
| `close_source(path)` command | Closes `path` if it is the open source: kills a running proxy build (waiting for ffmpeg to exit) and deletes its proxy and thumbnails. Any other path is ignored, so a close that raced a newer open cannot close it. Returns nothing. Removing the clip in the editor calls it. |
| `make_proxy(path)` command | Returns the proxy's path once built, or the existing one. `path` must be the open source. |
| `make_thumbnails(path)` command | Returns the paths of the open source's 24 timeline thumbnails in time order: the existing ones, or new ones made from the proxy once it is built and from the source otherwise. `path` must be the open source. |
| `proxy-progress` event | `{ source, progress }`, progress 0..1 from ffmpeg's `out_time_us` over the probed duration. |
| `PreviewError` | Tagged by `kind`: `notFound`, `invalidSourceInfo`, `notServable`, `notOpen`, `ffmpegMissing`, `proxyFailed` and `thumbnailsFailed` (ffmpeg's last stderr line as `reason`), `cancelled` (the frontend ignores it). |

**Source or proxy** ([why](../decisions/2026-11-24-preview-source-or-proxy.md)): `choose_preview_source` plays the source when all three hold:

- the video codec is H.264, VP9 or AV1. HEVC always takes the proxy: WebView2 decodes it only with the HEVC Video Extensions and a capable GPU, so it played on the owner's machine but cannot be counted on elsewhere;
- the container is MP4 (`format_name` lists `mp4`, which also covers MOV) or Matroska named `.mkv` or `.webm`. ffprobe names MKV and WebM alike (`matroska,webm`) and Chromium reads both with one demuxer, so the extension only limits direct play to the names tested; `.mka` or a misnamed Matroska file takes the proxy;
- the audio is absent or one of the container's confirmed codecs, `MP4_AUDIO_CODECS` and `MATROSKA_AUDIO_CODECS`: AAC, MP3, Opus, Vorbis and FLAC in both.

Each video codec and container, and each audio codec in each container, was confirmed on generated clips (`testsrc2` plus a `sine` tone) in Chrome and in the app's WebView2: decoded frames counted, seeks landing, `webkitAudioDecodedByteCount` growing, and the tone reaching Web Audio as non-silent samples before and after a seek. Everything else takes the proxy: DTS decoded in neither browser; AC-3 and E-AC-3 decoded in WebView2 on the owner's machine but not in Chrome (Edge uses the Windows Dolby decoder, which not every install has, so like HEVC they depend on the PC); PCM decoded in both but is outside the set the owner admitted. The audio check matters because undecodable audio raises no `error`: the video plays silently. The owner's 2.4 GB 4K H.264/FLAC MKV now opens in under half a second instead of a one-minute proxy, with sound, and seeks across it land in 0.3 to 0.8 s. When the player raises `error` on the source, or shows no picture (`videoWidth` 0), the frontend switches to a proxy.

**Proxy** (`proxy_args` in [preview/domain.rs](../../src-tauri/src/preview/domain.rs)): `-map 0:V:0` (skips cover art) plus `-map 0:a:0` when the source has audio; `scale` to at most 720 high from the displayed size, never upscaled, both sides even; `-fps_mode passthrough`; `libx264 -preset ultrafast -crf 23`; `-force_key_frames expr:gte(t,n_forced)` (a keyframe a second, for scrubbing); stereo AAC 128k; `-movflags +faststart`; `-progress pipe:1`. There is no `-ss`, `-copyts` or `-r`, so the proxy starts at 0 at the source's first timestamp and time t in the player is t after the source's start, the offset export's input `-ss` measures (a test checks this on a source starting at 2.5 s). The player at t shows the frame whose span holds t, while input `-ss t` starts at the first frame at or after t, up to one frame later; export seeks a frame early to match (*Exporting*). A 2.4 GB 4K H.264/FLAC MKV of 187 s took about a minute to proxy on the owner's machine, before MKV played directly.

**Thumbnails** (`thumbnail_args` in [preview/domain.rs](../../src-tauri/src/preview/domain.rs), after gif-studio's `makeStrip`): one ffmpeg pass writes `thumb-01.jpg` to `thumb-24.jpg`, 72 high, sampling the middle of each of 24 equal slices (input `-ss` of half a slice, then `fps=1/slice`; `tpad` repeats the last frame first, or `fps` stops short at the last decoded frame). Past 90 s it adds `-skip_frame nokey`, so each thumbnail is the last keyframe before its time. mgck seeks a hidden `<video>` 24 times instead; measured in Chrome, that took 2.8 s on the 4K MKV's 720p proxy but 20 s on a 120 s 4K H.264 MP4 with a keyframe every 10 s, where the ffmpeg pass takes 0.7 s. In the app, the strip appeared 0.8 s after that MP4 opened. A source played directly is its own thumbnail input: from the 2.4 GB 4K MKV the pass takes about 7 s, which is why a built proxy is preferred. The frontend shows a plain bar until they arrive, and when the command fails.

**Asset protocol**: the webview loads sources, proxies and thumbnails from `http://asset.localhost/<path>` (`convertFileSrc`). [tauri.conf.json](../../src-tauri/tauri.conf.json) enables the protocol (Cargo feature `protocol-asset`) with an empty scope; [preview/presentation.rs](../../src-tauri/src/preview/presentation.rs) allows this session's temp folder and its subfolders at startup and each source file as it opens, and any other path gets 403. Responses carry `Access-Control-Allow-Origin` for the app's origin, so the editor's `<video>` elements set `crossOrigin='anonymous'`: without it their frames taint the output preview's canvas and it draws nothing. Tauri's scope cannot shrink, so a session's earlier sources stay readable until exit. Range requests get 206 with at most 1000 KB per response, and the `<video>` element always sends `Range`: a 2.75 GB file opened and seeked with the app's memory under 50 MB. A request without `Range` reads the whole file into memory, so anything else fetching a source sends one.

**Temp folder** ([temp_folder.rs](../../src-tauri/src/temp_folder.rs), in the shared kernel; the preview slice creates and deletes it): `%TEMP%\video-to-gif\session-<pid>\` holds `proxy-<n>.mp4` files, `thumbs-<n>` folders, export's `captions-<n>.png` while an export runs, and a `lock` file the running app keeps locked. At startup the app deletes every entry in `%TEMP%\video-to-gif` except session folders whose lock another running copy holds, so a crashed session's proxies go and a second running app's stay. A proxy and its source's thumbnails live while the source stays open; [session.rs](../../src-tauri/src/preview/infrastructure/session.rs) deletes them when another source replaces it or the editor removes its clip (`close_source`). On `RunEvent::Exit` (after the last window closes) the app cancels a running export (*Exporting*), kills a running proxy encode, waits for ffmpeg to exit so no file stays locked, then deletes its session folder and, when nothing else is in it, `%TEMP%\video-to-gif`.

To drive the preview in the real app, launch the debug build with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` and evaluate JS over CDP (`http://127.0.0.1:9222/json`); `window.__TAURI_INTERNALS__.invoke` reaches the commands. The debug build opens a console window beside the app; the release build does not.

## Exporting

The `export` slice turns an export request into outputs. IPC types are in [export/domain/](../../src-tauri/src/export/domain/) (`plan.rs` for the request, `outcome.rs` for results, `memory.rs` for the estimate), mirrored in [ipc/export.ts](../../src/ipc/export.ts) and, for the request itself, `ExportRequest` in [editor/lib/types.ts](../../src/editor/lib/types.ts); change both sides together.

| IPC | Shape |
| --- | --- |
| `run_export(request, destination)` command | `request` is `{ path, info, trim?, crop?, settings, keepSound, captions? }`: the source path, the `SourceInfo` `probe_source` returned (rechecked), the selection, the crop as fractions, mgck's `AdvancedSettings`, and the caption PNG as bytes. `destination` is an existing folder (VTG-7 picks the default). Resolves, once every ticked file type has settled, with `ExportOutput[]` in run order. |
| `cancel_export()` command | Kills the running export's ffmpeg; that output and those not yet started settle as `cancelled`. Returns at once. |
| `estimate_export_memory(request)` command | `request` is `{ info, trim?, crop?, settings }`. Returns `{ heldBytes, availableBytes, limitSeconds, fits }` (*Frame-memory guard*). |
| `export-progress` event | `{ fileType, progress }`, progress 0..1 per file type: ffmpeg's `out_time_us` over the output's expected seconds, 0 at its start and 1 when it is saved. GIF, Reverse and Boomerang hold frames until the input ends, so their progress sits near 0 and then jumps. |
| `export-output` event | One `ExportOutput` as it settles: `{ status: 'done', fileType, path, size }`, `{ status: 'failed', fileType, reason }` (the last lines of ffmpeg's stderr, or the filesystem's error) or `{ status: 'cancelled', fileType }`. |
| `ExportError` | Rejects `run_export` (or the estimate) before anything runs. Tagged by `kind`: `notFound`, `destinationNotFound`, `invalidRequest` (`reason`), `ffmpegMissing`, `notEnoughMemory` (`heldBytes`, `availableBytes`), `busy` (one export at a time), `captionsFailed`. |

**Validation** (`ExportRequest::validate` in [plan.rs](../../src-tauri/src/export/domain/plan.rs)): settings are clamped to mgck's `settingLimits` (width 16 to 4096, fps 1 to 60, GIF colours 2 to 256, WebP quality 1 to 100), speed to 0.25 to 3, plays to 0 to 65535; file types are sorted into GIF, WebP, MP4 and de-duplicated, and at least one is required. The selection must lie in the source and the crop in the frame. Rust works out the output size (`output_size`, mgck's `outputSize`) from `info`'s displayed size, the crop and the rotation, and the output length (`output_seconds`) from the selection, speed and direction; the frontend's own figures for these are not sent. A caption PNG must be exactly that size. Sound stays only when asked, the source has sound, and `sound_drop_reason` allows it (forward, 0.5× to 2×). Caption text and size are not read: the PNG carries them.

**Arguments** (`single_pass_args` in [args.rs](../../src-tauri/src/export/domain/args.rs)): mgck's `buildSinglePassCommand`, `cropFilter` and `trimInput`, with mgck's test cases as unit tests. One ffmpeg run per file type, filters in Gif Studio's order: crop (fractions of `iw`/`ih`, which ffmpeg evaluates on the auto-rotated frame), rotate, flips, `setpts` speed, `fps`, lanczos `scale`, caption overlay, then direction. GIF: `palettegen stats_mode=diff` and `paletteuse diff_mode=rectangle`, `-loop` as repeats. WebP: `libwebp_anim` ([why](../decisions/2026-11-24-webp-anim-encoder.md)), `-loop` as plays. MP4: libx264 CRF 20 `medium`, `yuv420p`, `+faststart`, and with sound `-map 0:a:0? -c:a aac -b:a 160k`, `atempo` at other speeds, `-shortest`. Every run is prefixed with `-hide_banner -loglevel error -nostats -progress pipe:1 -n -nostdin`, so stderr holds only errors and nothing is ever overwritten.

**Trim**: input `-ss` and `-t` before `-i`, so only the selection is decoded and encoded. `-ss` is `seek_start(start)`: the trim start minus one source frame (`1 / info.frameRate`) plus 1 ms, which keeps exactly the frame the player showed at the trim start (*Previewing a source*); plain `-ss start` started up to a frame late. A test exports from a clip whose frames carry their own number in their brightness and checks the first output frame. A variable-frame-rate source can still start a frame off where frames bunch up, since the step is the probe's average.

**Running** ([application.rs](../../src-tauri/src/export/application.rs)): `run` takes progress and output callbacks, which `run_export` turns into events, so the whole run is tested without a Tauri app. It refuses a run that does not fit in free memory, writes the caption PNG as `captions-<n>.png` in this session's temp folder ([temp_folder.rs](../../src-tauri/src/temp_folder.rs) `session_dir`), then runs the ticked file types one at a time in the order GIF, WebP, MP4, each through [process.rs](../../src-tauri/src/process.rs). A file type that fails is reported and the rest still run, as in mgck. The caption file is deleted when the run ends, however it ends. `Exports` (Tauri state) allows one export at a time; on app exit it cancels the running one and waits up to 5 s for it to clean up, before preview deletes the temp folder.

**Output files** ([outputs.rs](../../src-tauri/src/export/infrastructure/outputs.rs)): ffmpeg writes each output to `<stem>.part-<pid>-<n>.<ext>` in the destination folder (the extension stays last because ffmpeg picks the muxer from it). Only when ffmpeg exits with success is the file moved to its name, mgck's `singlePassDownloadName` (`dance.mov` becomes `dance.gif`); a taken name becomes `dance (1).gif`, `dance (2).gif` and so on. The move is `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`, which claims a name and fails if it exists in one step, so two runs finishing together never share a name and nothing is overwritten (`std::fs::rename` would replace). On a non-zero exit or a cancel, the `.part` file is deleted once ffmpeg has exited, so the folder only ever gains finished outputs (gif-studio's OSS-4 item 1). Only a crash or power cut can leave a `.part` file.

**Frame-memory guard** ([memory.rs](../../src-tauri/src/export/domain/memory.rs), gif-studio's OSS-4 item 3): mgck's `heldCopies` and `frameMemoryLimitSeconds`. A GIF queues every frame for its palette (one copy), `reverse` holds the whole selection (one more), and Boomerang adds its forward half to the GIF queue, at 4 bytes a pixel; forward WebP and MP4 stream and hold nothing. The budget is free physical memory, `GlobalMemoryStatusEx`'s `ullAvailPhys` ([infrastructure/memory.rs](../../src-tauri/src/export/infrastructure/memory.rs)), read when the estimate or the run asks; off Windows it is unknown and nothing is refused. `run_export` refuses with `notEnoughMemory` when the held bytes exceed it; `estimate_export_memory` gives the UI the same figures to warn with.

Integration tests in [application/tests.rs](../../src-tauri/src/export/application/tests.rs) run the real ffmpeg on clips generated at test time (`testsrc2`, `sine`, a frame-counting `geq` clip, a marked frame, a 90° `-display_rotation` copy) and check every file type with ffprobe and decoded frames: size, duration, loop count (the GIF's NETSCAPE extension, the WebP's ANIM chunk), sound, orientation, caption pixels, a cancelled and a failed run leaving nothing, and a numbered name. `cargo test` therefore also needs ffmpeg with libwebp_anim and libx264.

# Glossary

The project's ubiquitous language, one row per concept; the index's *Language* section says how to use it. Sorted by term.

| Term | Meaning | Avoid | In code |
| --- | --- | --- | --- |
| Advanced mode | The editor of [mgck's Discord GIF tool](https://mgck.ink/discordgifs) that gives every setting (trim, crop, size, speed, direction, captions, format options). This app reproduces it; mgck's simple mode with presets is later work. | expert mode, full editor | mgck's `AdvancedEditor.tsx` |
| Caption | Advanced mode's meme-style top or bottom line of text, drawn in Anton (or a system face for non-Latin text) at a share of the output's shorter side. The preview draws it; Convert renders it to a PNG at the exact output size for export to overlay. | text overlay, subtitle | `editor/lib/captions.ts` |
| Crop | The rectangle of the source's frame an export keeps, stored as fractions of the frame so it means the same on a proxy; its pixels are those fractions of the displayed size. | | `ClipCrop`, `VideoCropper.tsx` |
| Direction | How the selection plays in the output: forward, reverse, or boomerang (forward then backward, twice as long). The output preview always plays forward and labels the others. | | `AdvancedSettings.direction` |
| Displayed size | A source's width and height as shown: the coded size with rotation metadata applied, so a quarter turn swaps them. Crop rectangles and output sizes are in these pixels, because ffmpeg auto-rotates before any filter. | coded size (that is the pre-rotation size) | `SourceInfo.width`/`height` |
| Export | Running ffmpeg on the source, with the selection, crop and settings, to produce one or more outputs. Always reads the source, never the proxy. | convert, render, encode | `export` slice |
| Export request | What Convert hands to export: source path, selection, crop, settings, exact output width, whether to keep sound, caption image and output length. | job | `ExportRequest` in `editor/lib/types.ts` |
| File type | GIF, WebP or MP4, ticked in any mix under *Save as*; each ticked one becomes an output. mgck's name, kept because "output format" named a simple-mode preset there. | output format | `FileType` |
| Output | A GIF, WebP or MP4 file produced by an export. One export can produce any mix of the three; the results screen lists them with Open and Show in folder. | | |
| Output preview | The live canvas beside the player showing the selection as it will encode: crop, size, rotation, flips and captions, at the chosen speed. Drawn from the player's frames, so it follows a proxy too. | preview (alone, that is the player) | `AdvancedPreview.tsx`, `OutputPreview.tsx` |
| Preview source | What the editor's video player plays: the source itself when WebView2 can decode it, otherwise its proxy. | | `preview::domain::PreviewSource`, `open_source` command |
| Probe | Reading a source's metadata with ffprobe and validating it into source info. | inspect, analyse | `probe` slice, `probe_source` command |
| Proxy | A reduced H.264 copy of the source (at most 720 high, a keyframe a second), made by ffmpeg for preview only, never exported, kept in the temp folder while its source stays open. UI copy calls it a "preview copy", a phrase users understand. | transcode, preview file | `preview` slice, `make_proxy` command |
| Selection | The trimmed time range of the source that an export uses. | clip, trim range, range | |
| Setup message | What the app shows in place of its main view when `ffmpeg` or `ffprobe` is missing from PATH or does not run; it names which and how to install them. | error screen, onboarding | `SetupMessage.tsx`, `find_ffmpeg` command |
| Shared kernel | Crate-level Rust modules outside every slice that any slice may use: `process.rs` and `search_path.rs`, which find and start ffmpeg and ffprobe. | common, utils | `crate::process`, `crate::search_path` |
| Source | The user's video, opened by path through drag-drop or a file dialog; any file the user's ffmpeg can read. Never copied or modified. One is open at a time; opening another replaces it. | input, upload | `editor/useSource.ts` |
| Source info | A source's validated metadata: duration, displayed size, frame rate, video codec, container and whether it has audio. | metadata, probe result | `probe::domain::SourceInfo` |
| Temp folder | `%TEMP%\video-to-gif`, where the app keeps proxies: one session folder per running app, swept of crashed sessions at startup and deleted at exit. | cache, scratch dir | `preview::infrastructure::temp_folder` |
| Thumbnails | The 24 frames along the trim timeline, made by ffmpeg in one pass from the preview source and kept in the temp folder while the source is open. | strip, filmstrip | `make_thumbnails` command |

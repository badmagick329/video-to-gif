# Glossary

The project's ubiquitous language, one row per concept; the index's *Language* section says how to use it. Sorted by term.

| Term | Meaning | Avoid | In code |
| --- | --- | --- | --- |
| Advanced mode | The editor of [mgck's Discord GIF tool](https://mgck.ink/discordgifs) that gives every setting (trim, crop, size, speed, direction, captions, format options). This app reproduces it; mgck's simple mode with presets is later work. | expert mode, full editor | mgck's `AdvancedEditor.tsx` |
| Displayed size | A source's width and height as shown: the coded size with rotation metadata applied, so a quarter turn swaps them. Crop rectangles and output sizes are in these pixels, because ffmpeg auto-rotates before any filter. | coded size (that is the pre-rotation size) | `SourceInfo.width`/`height` |
| Export | Running ffmpeg on the source, with the selection, crop and settings, to produce one or more outputs. Always reads the source, never the proxy. | convert, render, encode | `export` slice |
| Output | A GIF, WebP or MP4 file produced by an export. One export can produce any mix of the three; the results screen lists them with Open and Show in folder. | | |
| Preview source | What the editor's video player plays: the source itself when WebView2 can decode it, otherwise its proxy. | | `preview` slice |
| Probe | Reading a source's metadata with ffprobe and validating it into source info. | inspect, analyse | `probe` slice, `probe_source` command |
| Proxy | A reduced H.264 copy of the source, made by ffmpeg for preview only, never exported. | transcode, preview file | `preview` slice |
| Selection | The trimmed time range of the source that an export uses. | clip, trim range, range | |
| Setup message | What the app shows in place of its main view when `ffmpeg` or `ffprobe` is missing from PATH or does not run; it names which and how to install them. | error screen, onboarding | `SetupMessage.tsx`, `find_ffmpeg` command |
| Source | The user's video, opened by path through drag-drop or a file dialog; any file the user's ffmpeg can read. Never copied or modified. | input, upload | |
| Source info | A source's validated metadata: duration, displayed size, frame rate, video codec, container and whether it has audio. | metadata, probe result | `probe::domain::SourceInfo` |

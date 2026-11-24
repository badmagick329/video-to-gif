# Glossary

The project's ubiquitous language, one row per concept; the index's *Language* section says how to use it. Sorted by term.

| Term | Meaning | Avoid | In code |
| --- | --- | --- | --- |
| Advanced mode | The editor of [mgck's Discord GIF tool](https://mgck.ink/discordgifs) that gives every setting (trim, crop, size, speed, direction, captions, format options). This app reproduces it; mgck's simple mode with presets is later work. | expert mode, full editor | mgck's `AdvancedEditor.tsx` |
| Export | Running ffmpeg on the source, with the selection, crop and settings, to produce one or more outputs. Always reads the source, never the proxy. | convert, render, encode | `export` slice |
| Output | A GIF, WebP or MP4 file produced by an export. One export can produce any mix of the three; the results screen lists them with Open and Show in folder. | | |
| Preview source | What the editor's video player plays: the source itself when WebView2 can decode it, otherwise its proxy. | | `preview` slice |
| Proxy | A reduced H.264 copy of the source, made by ffmpeg for preview only, never exported. | transcode, preview file | `preview` slice |
| Selection | The trimmed time range of the source that an export uses. | clip, trim range, range | |
| Source | The user's video, opened by path through drag-drop or a file dialog; any file the user's ffmpeg can read. Never copied or modified. | input, upload | |

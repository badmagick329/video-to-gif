# Real paths, no copies

Status: Accepted (2026-11-24).

## Context

The browser version receives a source as a `File` and copies it into ffmpeg.wasm's in-memory filesystem, one reason multi-GB recordings fail there. A desktop app can know where the source lives on disk.

## Decision

Sources arrive as filesystem paths, by native drag-drop onto the window or a file dialog. The app never copies a source.

## Consequences

- Source size is limited only by ffmpeg and the disk, not by memory.
- ffprobe, the proxy encode and the export all read the source in place by its path.
- The frontend handles a path, not file contents; playing the source in the webview needs Tauri to serve that path to it, settled in VTG-4.
- A source moved or deleted while open fails at its next read; Rust reports that as an error rather than assuming the path stays valid.

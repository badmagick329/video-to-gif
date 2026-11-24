# Rust owns ffmpeg

Status: Accepted (2026-11-24).

## Context

mgck builds ffmpeg arguments in TypeScript and runs ffmpeg.wasm in the page. gif-studio runs a local Node server that the page calls over HTTP. In a Tauri app the frontend could likewise build command lines and ask Rust to run them, or call a localhost server.

## Decision

The frontend sends the source path, the selection (trim), the crop and the settings over Tauri IPC. Rust builds the ffmpeg arguments, runs ffmpeg and ffprobe, and streams progress back as events. There is no localhost server.

## Consequences

- The argument building that mgck does in TypeScript moves to Rust's `export` slice, where it can be unit-tested without a webview.
- The IPC payload is a trust boundary: Rust validates it before starting a process, and the frontend can never run an arbitrary command line ([architecture](../context/architecture.md) *IPC boundary*).
- No port is opened, so nothing else on the machine can reach the app.
- Progress, cancel and results are Rust's job and reach the UI as Tauri events.

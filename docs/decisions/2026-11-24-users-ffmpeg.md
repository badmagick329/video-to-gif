# The user's ffmpeg

Status: Accepted (2026-11-24).

## Context

Every feature runs `ffmpeg` or `ffprobe`. The app could bundle them, download them on first run, or use the ones the user already has. Bundling and downloading add build, update and licensing work; using what is on PATH lets v0 start with none of it.

## Decision

v0 finds `ffmpeg` and `ffprobe` on PATH and shows a setup message when either is missing. Bundling or downloading ffmpeg is later work ([the v0 scope](2026-11-24-v0-scope.md)).

## Consequences

- The `probe` slice owns the lookup ([architecture](../context/architecture.md) *Finding ffmpeg*); nothing else works until both are found.
- The ffmpeg build is whatever the user installed, so versions and enabled encoders can differ between users. v0 is tested against the owner's ffmpeg 9.0.1 (gyan.dev full build).
- The app has no ffmpeg binaries to ship, license or update.

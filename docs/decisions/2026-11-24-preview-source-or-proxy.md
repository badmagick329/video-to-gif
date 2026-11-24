# Preview plays the source or a proxy

Status: Accepted (2026-11-24).

## Context

The editor needs a video player for trimming and cropping, and the player is WebView2 ([the Tauri decision](2026-11-24-tauri-2.md)). WebView2 decodes common formats such as H.264 MP4 but not every source ffmpeg reads: MKV containers, HEVC and some AV1 files can fail to play. Those are the sources the app exists for.

## Decision

Preview plays the source when WebView2 can decode it. Otherwise ffmpeg makes a proxy: a reduced H.264 copy used only for preview. Export always reads the source.

## Consequences

- The `preview` slice decides between the source and a proxy and makes the proxy ([architecture](../context/architecture.md) *Slices*).
- A proxy's resolution and quality never affect an output, so the proxy can be as small and fast to make as preview allows.
- Times and crop chosen on a proxy must map onto the source exactly, so the proxy keeps the source's timeline and aspect.
- Opening a source WebView2 cannot play costs a proxy encode before preview starts.

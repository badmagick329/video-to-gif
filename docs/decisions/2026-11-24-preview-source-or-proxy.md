# Preview plays the source or a proxy

Status: Accepted (2026-11-24), amended 2026-11-24 (see below).

## Context

The editor needs a video player for trimming and cropping, and the player is WebView2 ([the Tauri decision](2026-11-24-tauri-2.md)). WebView2 decodes common formats such as H.264 MP4 but not every source ffmpeg reads: MKV containers, HEVC and some AV1 files can fail to play. Those are the sources the app exists for.

## Decision

Preview plays the source when WebView2 can decode it. Otherwise ffmpeg makes a proxy: a reduced H.264 copy used only for preview. Export always reads the source.

## Consequences

- The `preview` slice decides between the source and a proxy and makes the proxy ([architecture](../context/architecture.md) *Slices*).
- A proxy's resolution and quality never affect an output, so the proxy can be as small and fast to make as preview allows.
- Times and crop chosen on a proxy must map onto the source exactly, so the proxy keeps the source's timeline and aspect.
- Opening a source WebView2 cannot play costs a proxy encode before preview starts.

## Amendment (2026-11-24): MKV plays directly, and audio codecs are checked

The owner asked for this: a 2.4 GB 4K H.264/FLAC MKV waited about a minute for a proxy although WebView2 plays H.264, VP9 and AV1 in MKV.

- Matroska named `.mkv` joins MP4 and `.webm` as a container played directly. H.264/FLAC, H.264 with each allowed audio codec, and VP9 and AV1 with FLAC all played in the app's WebView2, picture and sound.
- The source plays directly only when its audio is absent or a codec confirmed for its container (AAC, MP3, Opus, Vorbis, FLAC in MP4 and Matroska). Audio the player cannot decode raises no error: the video plays silently, so the frontend's fallback never fires. DTS decoded nowhere; AC-3 decoded in WebView2 on the owner's machine through the Windows Dolby decoder but not in Chrome, so like HEVC it depends on the PC and takes the proxy. Checking the codec up front closes that gap, which already existed for MP4.
- The allowlist and how each pair was confirmed are in [architecture](../context/architecture.md) *Previewing a source*.

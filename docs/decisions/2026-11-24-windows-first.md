# Windows first

Status: Accepted (2026-11-24).

## Context

The owner uses Windows and has no macOS or Linux machine to test on. Tauri, Rust and ffmpeg all run on the three platforms, so most of the code is portable whether or not anyone tests it.

## Decision

- Windows is the only platform that is built and tested.
- Code stays cross-platform where that costs nothing: paths go through `std::path`, and Windows-only APIs sit behind `#[cfg(windows)]`.
- macOS and Linux fixes wait for user reports on GitHub.

## Consequences

- A Windows-only behaviour needs a `cfg` gate and, where the other platforms need something, a fallback; it never breaks their build on purpose.
- WebView2 is the only webview whose codec support is checked ([the preview decision](2026-11-24-preview-source-or-proxy.md)); WebKit on macOS and Linux may play a different set.
- macOS and Linux testing is listed as later work in [the v0 scope](2026-11-24-v0-scope.md).

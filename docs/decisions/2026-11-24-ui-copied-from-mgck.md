# The editor UI is a copy of mgck's

Status: Superseded (2026-11-24) by [the editor UI is the app's own desktop layout](2026-11-24-editor-ui-from-canvas.md). The settings code copied here stays; the layout no longer follows mgck.

## Context

mgck's [Discord GIF tool](https://mgck.ink/discordgifs) already has the advanced-mode editor this app reproduces, in React, TypeScript and Tailwind (`C:\code\python\web\mgck`; paths in the [index](../README.md) *Working rules for agents*). It runs ffmpeg.wasm in the browser, so its UI code is interleaved with browser and Wasm concerns this app does not have. Sharing a package between the two would tie a Next.js web app and a Tauri desktop app to one release cycle.

## Decision

The advanced-mode components and logic are copied from mgck into this repo and adapted. From then on the two codebases drift independently; there is no shared package. mgck's repo is reference only and stays unchanged.

## Consequences

- A fix or feature in one app reaches the other only by being ported by hand.
- mgck's comments about Wasm memory and missing encoders describe limits this app does not have; the adapted code drops those limits and their workarounds.
- mgck's behaviour is the feature spec within [the v0 scope](2026-11-24-v0-scope.md), so a question about how a setting behaves is answered by reading mgck first.

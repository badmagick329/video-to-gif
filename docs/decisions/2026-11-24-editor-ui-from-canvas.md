# The editor UI is the app's own desktop layout, designed on the canvas

Status: Accepted (2026-11-24). Supersedes [the editor UI is a copy of mgck's](2026-11-24-ui-copied-from-mgck.md).

## Context

v0 copied mgck's advanced-mode editor: a web page of stacked cards that scrolls, built for a browser tab and Discord's upload limits. On a desktop that layout wastes the window, and its copy judged sizes against Discord. Epic VTG-9 set out to make the app look and behave like a desktop video editor, and the owner reviewed and approved a set of canvas boards for it (`Main.dc.html`, `Start.dc.html`, `Exports.dc.html`, `Estimate.dc.html`; VTG-9 links them and says how to read them).

## Decision

The editor's layout and look follow the canvas boards, and mgck stops being the UI spec. The window is fixed and never scrolls as a page: a toolbar, a viewer over a timeline area, an inspector docked on the right, and a status bar ([architecture](../context/architecture.md) *Frontend*). The palette, type and spacing are the boards'.

Settings keep the behaviour they were ported with. mgck stays the reference for how a setting was built, and the read-only source to compare against when a setting's behaviour is in question.

## Consequences

- A UI change is checked against the boards, not against mgck. Sizes, timings and file names on the boards are illustrative.
- Code names drop mgck's "advanced" (there is no simple mode): the settings are `OutputSettings`, and the glossary lists *advanced mode* under Avoid.
- A new UI feature that the boards do not show needs the owner's go-ahead, as a feature outside the scope does.
- The layout is checked in headless Edge against the Vite frontend with IPC mocked; the real window (WebView2 in the Tauri frame) still needs its own look.

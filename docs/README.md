# Document index

These documents live under `docs/` and are tracked in Git: commit document changes with the work they describe, and keep credentials and unneeded personal data out of them. Read only what the task needs. Current user decisions, code, configuration and maintained public docs outrank these documents: when one disagrees with a document, correct the document; when two of them disagree with each other, ask the user.

## Project in one paragraph

video-to-gif is a Windows-first desktop app that turns a video into GIF, WebP and MP4 with everything the advanced mode of [mgck's Discord GIF tool](https://mgck.ink/discordgifs) offers, by running the user's own `ffmpeg` and `ffprobe` from PATH. Because ffmpeg runs natively, it takes any source that ffmpeg can read, at any size, including the AV1, HEVC and MKV files and multi-GB recordings the browser version fails on. It is Tauri 2: a Rust backend that owns ffmpeg, and a React + TypeScript + Vite + Tailwind frontend in WebView2, talking over Tauri IPC. The root `README.md` is the public page. Status: v0 (epic VTG-1) is done and released as 0.1.0, an unsigned per-user NSIS installer ([releasing](context/releasing.md)) that the owner installed and used to convert an AV1 clip and a 2.4 GB recording. The app is a fixed-window desktop editor over a source opened in place, played directly or through a proxy: a toolbar, a viewer with the source and the live output side by side, a zoomable timeline dock that sets the selection to the frame, an inspector with every output setting over a pinned export footer, and a status bar. Export runs Rust's export with a frame-memory warning that never blocks; the inspector's Exports tab shows each file type's progress during a run and lists the session's outputs with Open, Show in folder and Copy (to the clipboard as a file). Current work is epic VTG-9, the desktop editor redesign: VTG-10 removed mgck's Discord-era copy and added a size estimate per file type from background sample encodes, VTG-11 built the editor shell from the canvas boards, VTG-12 the timeline dock, and VTG-13 the Exports tab; the start state and gifski follow.

## The plan: YouTrack project `VTG`

The plan and all issues live in YouTrack project `VTG` ("video-to-gif"), read and updated through the `youtrack` MCP tools. The current epic is **VTG-9** (desktop editor redesign): it holds the payoff, the decisions settled with the owner on 2026-11-24, the canvas boards the UI follows, the scope and the ordered list of child Tasks. **VTG-1**, the finished v0 epic, holds the v0 decisions (recorded here as ADRs), the reference repos and the v0 scope. Open work is `project: VTG #Unresolved`; the Task being worked is the one in state In Progress. Each Task holds its Steps as a checklist and ends on a **Done when** line.

- Work the Tasks in the current epic's order, one at a time.
- When a Step lands, tick it in the Task's description and add a comment saying what changed and in which commit.
- Resolve a Task once its Done when line is verified, then move the next Task to In Progress.
- To change a decision or the scope in an epic, ask the user first. Then record the change in that epic with its date and reason, and supersede the matching ADR here.

## Working rules for agents

- Commit with Conventional Commits messages, using the `conventional-commits` skill.
- Record project facts, decisions and the owner's preferences in this collection rather than in agent memory, so the owner can read and edit them.
- [The v0 scope decision](decisions/2026-11-24-v0-scope.md) and the current epic decide what gets built; the canvas boards are the UI spec ([why](decisions/2026-11-24-editor-ui-from-canvas.md)), and mgck is the reference for how each setting behaves. When a feature is in none of them, ask before building it.
- The reference repos are read-only: read them, never edit them.
  - **mgck**, the web version whose advanced-mode settings this app ported: `C:\code\python\web\mgck`. Advanced mode is `frontend/src/app/discordgifs/_components/AdvancedEditor.tsx`; the rest is in that folder, `frontend/src/lib/discordgifs/` and `frontend/src/hooks/discordgifs/`, with tests in `frontend/src/__tests__/discordgifs-*`. It runs ffmpeg.wasm, so its comments about Wasm memory and missing encoders describe limits this app does not have.
  - **gif-studio**, a friend's local-ffmpeg GIF tool whose filter chain mgck's advanced mode ported: `C:\code\oss\repos\gif-studio` (v1.1.1, `0ce6130`), one `server.js` plus `index.html`. Prior art for spawning ffmpeg with `-progress pipe:1`, preview proxies, timeline strips and job handling. Its known defects are listed in YouTrack OSS-4; each VTG Task names the ones it must avoid.

## Where to look

| Location | Purpose and retention |
| --- | --- |
| [context/glossary.md](context/glossary.md) | The ubiquitous language. Update when a concept is added, renamed or retired. |
| [context/architecture.md](context/architecture.md) | Repo shape, the frontend (window, viewer, inspector, keyboard, what came from mgck), slices and layers, the shared kernel, the IPC boundary, finding ffmpeg, previewing a source (allowlist, proxy, thumbnails, asset protocol, temp folder), exporting (commands and events, arguments, progress, trim seek, output files, frame-memory guard), estimating sizes (methods, the shared sample run, scheduling, the accuracy harness), saving outputs (the run in the editor, the running card, the session list, the remembered destination, Open, Show in folder and Copy, the memory warning). Rewrite a section when the code it describes changes; add one when a Task builds something new. |
| [context/releasing.md](context/releasing.md) | Cutting a release: the version's three homes, `pnpm tauri build` and its installer path, the NSIS bundle settings (per-user, WebView2 bootstrapper), regenerating the icon from its SVG, and signing and auto-update as later work. Update when the build, bundle settings or icon source change. |
| [decisions/2026-11-24-v0-scope.md](decisions/2026-11-24-v0-scope.md) | What v0 builds and what waits. Supersede with a new ADR when scope changes. |
| [decisions/2026-11-24-tauri-2.md](decisions/2026-11-24-tauri-2.md) | Why Tauri 2 with a React frontend, over Electron and a native Rust GUI. Supersede if the app framework changes. |
| [decisions/2026-11-24-windows-first.md](decisions/2026-11-24-windows-first.md) | Windows is the only tested platform; other platforms cost nothing extra. Supersede when another platform is tested. |
| [decisions/2026-11-24-users-ffmpeg.md](decisions/2026-11-24-users-ffmpeg.md) | v0 runs `ffmpeg` and `ffprobe` from PATH. Supersede when ffmpeg is bundled or downloaded. |
| [decisions/2026-11-24-editor-ui-from-canvas.md](decisions/2026-11-24-editor-ui-from-canvas.md) | The editor UI is the app's own desktop layout from the canvas boards; mgck stays the reference for setting behaviour. Supersede if the UI spec moves. |
| [decisions/2026-11-24-ui-copied-from-mgck.md](decisions/2026-11-24-ui-copied-from-mgck.md) | Superseded: v0 copied mgck's editor UI. Kept for the rationale behind the copied settings code. |
| [decisions/2026-11-24-rust-owns-ffmpeg.md](decisions/2026-11-24-rust-owns-ffmpeg.md) | The frontend sends intent over IPC; Rust builds arguments and runs ffmpeg. Supersede if that split changes. |
| [decisions/2026-11-24-preview-source-or-proxy.md](decisions/2026-11-24-preview-source-or-proxy.md) | Preview plays the source or a proxy; export always reads the source. Supersede if preview changes. |
| [decisions/2026-11-24-real-paths.md](decisions/2026-11-24-real-paths.md) | Sources arrive as filesystem paths and are never copied. Supersede if sources arrive another way. |
| [decisions/2026-11-24-vertical-slices.md](decisions/2026-11-24-vertical-slices.md) | Why the Rust crate is vertical slices with inward-pointing layers, when a slice gains a layer, and what the shared kernel holds. Supersede if the layout changes. |
| [decisions/2026-11-24-webp-anim-encoder.md](decisions/2026-11-24-webp-anim-encoder.md) | Why WebP export uses libwebp_anim over mgck's libwebp, with the size and SSIM/PSNR comparison. Supersede if the WebP encoder changes. |
| YouTrack project `VTG` ("video-to-gif") | The plan, Tasks, bugs and deferred work; see *The plan* above. |

## Language

[context/glossary.md](context/glossary.md) is the project's ubiquitous language. Name things with its terms in conversation, code, UI copy, docs and commits. A new concept gets its row in the same change that introduces it. When two names compete or a term stops fitting, ask the user, then rename everywhere or move the old name under *Avoid*. Terms copied from mgck's code keep their meaning here unless the glossary says otherwise.

## Where new documents go

Before writing, search the index and documents for the topic; extend the owning document when one exists. Load the `writing-for-agents` skill before creating or editing any document here, and the `humanizer` skill before editing the root `README.md`.

- `context/`: current architecture, conventions, operations and gotchas. One home per topic; point to code and configuration rather than copying them.
- `decisions/YYYY-MM-DD-topic.md`: ADRs with status (`Proposed`, `Accepted`, `Superseded`), context, decision and consequences. They record rationale, not live state. Mark a superseded ADR and link its replacement.
- `plans/active/`: substantial unfinished work that no VTG Task already describes, with date, status, scope and acceptance criteria. On completion, move lasting rationale to `decisions/` and current guidance to `context/`, then delete the plan, or move it to `plans/completed/` while its evidence still matters.
- `research/` and `audits/`: dated findings with sources and status. An audit finding closes only on execution evidence.
- `artifacts/<task>/`: temporary outputs and helpers. Delete them once the task is verified.

A bug, idea or backlog item the user decides to defer becomes an issue in YouTrack project `VTG`, not a document. Raise findings, proposals and questions in chat as usual; open an issue only for work the user has chosen to defer.

Create a folder only when it receives a document. This index is the only file at the docs root.

## Keeping it current

A change is finished only when these documents describe it. Before reporting or committing:

1. **Find.** Search this collection for the names, paths, commands and terms the change touched. Documents you never opened go stale too. Done when every hit is updated or confirmed still true.
2. **Replace.** Rewrite changed guidance where it lives, following `writing-for-agents`, so each section reads as current truth on its own. Keep the *Status* sentence in *Project in one paragraph* true.
3. **One home.** Each fact lives in the document the index names for its topic; everywhere else links to it. Merge any copies the change created or exposed.
4. **Lifecycle.** Plans show actual progress, and finished plans leave `plans/active/`. Delete consumed handovers, status snapshots and verified artifacts once their unique facts have moved. Ask before deleting user-authored or uncertain material.
5. **Language.** Every concept the change introduced or renamed has its glossary row, and no avoided name came back.
6. **Index.** Rows and inbound links match every added, moved or deleted path.
7. **Issues.** Every VTG Step the change landed is ticked, with a comment saying what changed and in which commit; every Task whose Done when line the change verified is resolved.

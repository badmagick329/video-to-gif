# Vertical slices over clean architecture

Status: Accepted (2026-11-24).

## Context

The Rust crate has three features from the start (probing a source, previewing it, exporting it) and external I/O in each: ffmpeg and ffprobe processes, the filesystem and the webview over IPC. The owner's standing rule is that a project with two or more features plus external I/O is structured as vertical slices over clean architecture. The same layout serves the owner's superslide-rs (`C:\code\rust\superslide-rs\docs\decisions\2026-10-10-vertical-slices.md`, outside this repo).

## Decision

- One folder per feature under `src-tauri/src/`: `probe`, `preview`, `export`. [architecture.md](../context/architecture.md) *Slices* says what each owns.
- Each slice holds only the layers it uses, from `domain`, `application`, `infrastructure` and `presentation`. A layer is created when code needs it, not up front. A use case starts as a call from `presentation` into `infrastructure`; it becomes the slice's `application` when it coordinates several steps or needs testing without its I/O, with a trait standing in for the I/O only if a test needs one.
- A slice's `#[tauri::command]`s live in its `presentation` module.
- `src-tauri/src/lib.rs` is the shell: it builds the Tauri app and is the only place that registers every slice's commands.
- Dependencies point inward. Within a slice, outer layers use inner ones. Across slices, only `domain` is shared, and no slice imports the shell.
- `domain` takes no I/O crates; `serde` derives are allowed there, so a domain type can cross IPC without a mirror type.
- Module paths name the layer (`crate::export::domain::…`), and there are no re-exports, so a wrong-way import shows in the `use` line.

## Consequences

- A Task's Rust work touches one slice folder plus, for a new command, `lib.rs`.
- Rules such as building ffmpeg arguments live in `domain` and are tested without spawning a process; tests that run ffmpeg sit in `infrastructure`.
- `export` and `preview` both need what `probe` learns about a source, so that type lives in `probe::domain`, which they may import.

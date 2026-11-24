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

## Amendment (2026-11-24): a shared kernel for spawning ffmpeg

`probe`, `preview` and later `export` all find and start ffmpeg or ffprobe, and each must run the same program the startup check reported and hide the console window. Copies in each slice would drift; importing from `probe` would break the cross-slice rule.

- Crate-level modules beside the slices form a **shared kernel**: [`process.rs`](../../src-tauri/src/process.rs) (start with no console window and a null stdin) and [`search_path.rs`](../../src-tauri/src/search_path.rs) (find `ffmpeg` and `ffprobe` on PATH as Windows does). Any slice's `infrastructure` may use them.
- The shared kernel imports no slice and holds mechanism only; rules about what to run stay in the slice that owns them.
- Across slices, a slice now shares its `domain` and the shared kernel, nothing else.

# gifski from PATH, optional

Status: Accepted (2026-11-25).

## Context

ffmpeg's `palettegen`/`paletteuse` makes one palette for the whole GIF. gifski builds a palette per frame and dithers across frames, which gives the best GIFs the app can make, and the owner asked for it as a choice of GIF encoder (epic VTG-9, Task VTG-15). gifski is AGPL-3.0: linking the crate or shipping the program would put the app under the AGPL. The app already runs the user's own ffmpeg from PATH ([the user's ffmpeg](2026-11-24-users-ffmpeg.md)).

## Decision

The app runs the user's own `gifski` from PATH, the way it runs ffmpeg, and never links the crate or bundles the program. gifski is optional: the startup check looks for it, a missing gifski is a normal state, and the GIF encoder select offers it only when it was found. ffmpeg stays the decoder: it runs the export's filters and pipes Y4M frames to gifski's stdin.

## Consequences

- The `probe` slice's startup check owns the lookup ([architecture](../context/architecture.md) *Finding tools*); the export slice looks gifski up again for each run, like ffmpeg.
- The gifski build is whatever the user installed. The pipe was verified with gifski 1.32.0, which reads Y4M from stdin, decodes it as BT.709, caps output at about 800 × 600 without `-W`, and resamples to its own `--fps` (*Exporting* in the architecture).
- A gifski GIF needs two processes per run; a job owns both, cancel kills both, and a failure names whichever process failed first.
- The app has no gifski binary to ship, license or update.

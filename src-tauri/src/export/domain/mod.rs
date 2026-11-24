//! The export slice's rules, with no I/O: settings and their limits, request validation, output
//! sizes and names, the ffmpeg argument builder and reading its progress, the frame-memory guard,
//! the size estimate's methods, and what crosses IPC.

pub mod args;
pub mod estimate;
pub mod memory;
pub mod outcome;
pub mod plan;
pub mod progress;
pub mod settings;

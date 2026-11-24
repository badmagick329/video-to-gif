//! The export slice's rules, with no I/O: settings and their limits, request validation, output
//! sizes and names, the ffmpeg argument builder, the frame-memory guard, and what crosses IPC.

pub mod args;
pub mod memory;
pub mod outcome;
pub mod plan;
pub mod settings;

//! Probe slice: finds `ffmpeg` and `ffprobe` on the user's PATH and reads a source video's
//! metadata (duration, displayed size, frame rate, codec, container, audio) through `ffprobe`.
//!
//! `ffprobe` output is untrusted input, so this slice validates it before anything else sees it.

pub mod domain;
pub mod infrastructure;
pub mod presentation;

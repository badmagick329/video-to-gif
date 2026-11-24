//! Probe slice: finds `ffmpeg` and `ffprobe` on the user's PATH and reads a source video's
//! metadata (duration, dimensions, frame rate, codecs) through `ffprobe`.
//!
//! `ffprobe` output is untrusted input, so this slice validates it before anything else sees it.

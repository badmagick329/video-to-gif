//! Export slice: turns a source, its trim selection, crop and output settings into `ffmpeg`
//! jobs for GIF, WebP and MP4, runs them one at a time, streams their progress to the frontend as
//! events, and puts each finished output in the destination folder without overwriting anything.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

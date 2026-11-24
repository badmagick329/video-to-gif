//! Preview slice: decides whether WebView2 can play a source as-is and, when it cannot, makes a
//! playable proxy with `ffmpeg` so the user can trim and crop against it. Owns the app's temp
//! folder, where proxies live, from startup sweep to deletion at exit.

pub mod domain;
pub mod infrastructure;
pub mod presentation;

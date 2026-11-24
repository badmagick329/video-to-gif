//! Application shell: builds the Tauri app and registers every slice's commands.
//!
//! Each feature is a vertical slice with its own folder; its `#[tauri::command]`s live in its
//! `presentation` module. This file is the only place that wires them into the app.

mod export;
mod preview;
mod probe;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Register slice commands here, and only here, as they are written, e.g.
        // .invoke_handler(tauri::generate_handler![
        //     probe::presentation::probe_source,
        //     export::presentation::start_export,
        // ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

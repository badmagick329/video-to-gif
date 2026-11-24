//! Application shell: builds the Tauri app and registers every slice's commands.
//!
//! Each feature is a vertical slice with its own folder; its `#[tauri::command]`s live in its
//! `presentation` module. This file is the only place that wires them into the app.
//! `process`, `search_path` and `temp_folder` are the shared kernel: every slice that runs ffmpeg
//! or ffprobe finds and starts it through them, and keeps its temporary files in the session's
//! temp folder.

#[cfg(test)]
mod estimate_accuracy;
mod export;
mod preview;
mod probe;
mod process;
mod search_path;
mod temp_folder;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            export::presentation::set_up(app)?;
            preview::presentation::set_up(app)
        })
        // Register slice commands here, and only here, as they are written.
        .invoke_handler(tauri::generate_handler![
            probe::presentation::find_ffmpeg,
            probe::presentation::probe_source,
            preview::presentation::open_source,
            preview::presentation::close_source,
            preview::presentation::make_proxy,
            preview::presentation::make_thumbnails,
            export::presentation::run_export,
            export::presentation::cancel_export,
            export::presentation::estimate_export_memory,
            export::presentation::estimate_output_size,
            export::presentation::cancel_size_estimate,
            export::presentation::saved_destination,
            export::presentation::remember_destination,
            export::presentation::open_output,
            export::presentation::show_output_in_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Exit follows the last window closing (or any other way the app quits), and runs
            // before the process ends: the last chance to stop ffmpeg and delete partial outputs
            // and proxies. Export first, so its caption image is gone before preview deletes the
            // temp folder it sits in.
            if let tauri::RunEvent::Exit = event {
                export::presentation::shut_down(app);
                preview::presentation::shut_down(app);
            }
        });
}

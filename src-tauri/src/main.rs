// Release builds run as a GUI app, so Windows opens no console window beside it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    video_to_gif_lib::run()
}

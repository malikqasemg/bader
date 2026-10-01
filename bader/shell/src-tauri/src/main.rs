// Bader runs without a console window: Bader is the whole UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    bader_lib::run()
}

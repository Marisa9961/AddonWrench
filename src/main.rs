#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod steam;
mod ui;

fn main() {
    ui::run();
}

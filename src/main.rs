#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use robocam::{cameras, engine, i18n, settings, updates};
#[cfg(windows)]
mod vision;
#[cfg(windows)]
mod windows;

fn main() {
    #[cfg(windows)]
    windows::run();
    #[cfg(not(windows))]
    eprintln!("robocam requires Windows 11. The control engine tests are portable.");
}

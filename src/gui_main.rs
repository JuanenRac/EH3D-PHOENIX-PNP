// EH3D-PHOENIX-PNP - the window, as a program of its own.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `phoenix-pnp-gui` opens the window with the files named on the command line. It is built with `--features gui`. On Windows it is a program of the graphical subsystem, so no console
//! window opens behind it; for that reason it prints nothing, and an error that keeps the window from opening is written to `last_error.txt` beside the settings of the window. The
//! command line (`phoenix-pnp`) keeps its console, so its output and its exit codes work in a terminal and in a script.
#![forbid(unsafe_code)]
#![cfg_attr(windows, windows_subsystem = "windows")]

use eh3d_phoenix_pnp::gui::{settings, shot, window};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // the picture mode writes nothing beside the picture, so nothing goes to the folder of the settings
    if shot::wants_picture(&args) {
        return ExitCode::from(window::take_picture(&args));
    }
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    match window::run(&files) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            settings::report_error(&format!("the window could not open: {e}"));
            ExitCode::from(1)
        }
    }
}
